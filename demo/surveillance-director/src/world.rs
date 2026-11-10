//! The demo world: every party, in memory, and the actions the presenter triggers.
//!
//! Time is simulated. The demo clock starts at 08:00:00 and advances 10 seconds each time the
//! cameras record a segment; every full minute each camera's node seals and anchors its batch.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use chrono::{DateTime, Utc};
use sthira_core::iso_micros;
use sthira_ledger::{Anchor, AnchorLedger, ArchiveAttempt, ChainSpec, SimLedger};
use sthira_node::{Batch, ChainNode};
use sthira_profile_surveillance::{
    Camera, Clip, Footage, PROFILE, SEGMENT_MS, Scene, SceneScript, SegmentCheck, SegmentRecord, SegmentStatus,
    TEMPLATE, check_segments, synth_bytes, tamper,
};
use sthira_verify::{
    EvidencePackage, PackageReport, PackagedRecord, VerifyReport, build_package, verify_package, verify_trail,
};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::views::*;

// ── Cast. Rename freely. ──
pub struct CameraCfg {
    pub id: &'static str,
    pub name: &'static str,
    pub zone: &'static str,
}

pub const SITE: &str = "WH-PUNE-01";
/// The 3PL warehouse operator: owns the cameras and the recorder, submits every anchor.
pub const WH3PL: &str = "WH3PL";
/// The counterparty that co-signs every anchor. Who it is depends on the selected role.
pub const CPTY: &str = "CPTY";
/// The vendor (goods owner): verifies from an evidence package; optionally promoted to member.
pub const VENDOR: &str = "VENDOR";
pub const CAMERAS: [CameraCfg; 3] = [
    CameraCfg { id: "CAM-DOCK", name: "Receiving dock", zone: "dock" },
    CameraCfg { id: "CAM-CAGE", name: "High-value cage", zone: "cage" },
    CameraCfg { id: "CAM-DISPATCH", name: "Outbound dock", zone: "dispatch" },
];
pub const CONSIGNMENT: &str = "C-1047";

/// Segments per batch: one minute of footage.
pub const SEGMENTS_PER_BATCH: i64 = 6;
pub const MINUTE_MS: i64 = SEGMENT_MS * SEGMENTS_PER_BATCH;
/// Minutes of anchored history the demo opens with.
pub const SEED_MINUTES: i64 = 30;
/// Auto-record runs at ×10: one 10-second segment per real second.
pub const AUTO_INTERVAL: Duration = Duration::from_secs(1);
/// Auto-record switches itself off after an hour of demo time, so a forgotten tab cannot fill memory.
const AUTO_MAX_TICKS: u32 = 360;
const ACTIVITY_KEPT: usize = 100;
const ACTIVITY_SHOWN: usize = 50;
const SYNC_SHOWN: usize = 12;

/// An error the UI shows to the presenter, with its HTTP status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DemoError {
    pub status: u16,
    pub message: String,
}

impl DemoError {
    pub fn bad(msg: impl Into<String>) -> Self {
        Self { status: 400, message: msg.into() }
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self { status: 404, message: msg.into() }
    }
    pub fn conflict(msg: impl Into<String>) -> Self {
        Self { status: 409, message: msg.into() }
    }
}

impl std::fmt::Display for DemoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for DemoError {}

pub type Result<T> = std::result::Result<T, DemoError>;

/// One camera at the 3PL: the device, the node that anchors its chain, and the recorder's storage.
struct Cam {
    cfg: &'static CameraCfg,
    camera: Camera,
    node: ChainNode<SegmentRecord>,
    footage: Footage,
    next_seq: u64,
}

/// What the 3PL handed to the vendor for one camera: the package and the clips of the window.
struct Handover {
    cam: String,
    package: EvidencePackage<SegmentRecord>,
    clips: Footage,
}

pub type Shared = Arc<Mutex<World>>;

pub struct World {
    ledger: Box<dyn AnchorLedger>,
    cams: Vec<Cam>,
    role: CounterpartyRole,
    vendor_member: bool,
    /// Demo time at the start of every chain, and now (Unix milliseconds).
    start_ms: i64,
    clock_ms: i64,
    incident: Option<IncidentView>,
    incidents_raised: u32,
    handover: Vec<Handover>,
    export: Option<ExportView>,
    auto_task: Option<JoinHandle<()>>,
    auto_ticks: u32,
    last_verify: Option<LastVerify>,
    activity: VecDeque<Activity>,
    version: u64,
    version_tx: watch::Sender<u64>,
}

fn short(h: &str) -> &str {
    &h[..h.len().min(8)]
}

fn plural(n: usize, w: &str) -> String {
    format!("{n} {w}{}", if n == 1 { "" } else { "s" })
}

/// `08:21:10` from an RFC 3339 timestamp.
fn hms(t: &str) -> &str {
    t.get(11..19).unwrap_or(t)
}

/// `08:21` from an RFC 3339 timestamp.
fn hm(t: &str) -> &str {
    t.get(11..16).unwrap_or(t)
}

fn parse_ms(t: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(t).ok().map(|d| d.timestamp_millis())
}

/// Today at 08:00:00 UTC: where the demo clock starts.
fn demo_start_ms() -> i64 {
    Utc::now().date_naive().and_hms_opt(8, 0, 0).expect("valid time").and_utc().timestamp_millis()
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    /// A freshly seeded network.
    pub fn new() -> Self {
        let (version_tx, _) = watch::channel(0);
        let mut w = Self {
            ledger: Box::new(SimLedger::new()),
            cams: Vec::new(),
            role: CounterpartyRole::default(),
            vendor_member: false,
            start_ms: 0,
            clock_ms: 0,
            incident: None,
            incidents_raised: 0,
            handover: Vec::new(),
            export: None,
            auto_task: None,
            auto_ticks: 0,
            last_verify: None,
            activity: VecDeque::new(),
            version: 0,
            version_tx,
        };
        w.reset();
        w
    }

    pub fn shared() -> Shared {
        Arc::new(Mutex::new(Self::new()))
    }

    /// Notified with the new version whenever something changes.
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.version_tx.subscribe()
    }

    fn bump(&mut self) {
        self.version += 1;
        self.version_tx.send_replace(self.version);
    }

    fn log(&mut self, level: Level, actor: &str, text: impl Into<String>) {
        self.activity.push_front(Activity {
            at: iso_micros(self.clock_ms),
            level,
            actor: actor.to_string(),
            text: text.into(),
        });
        self.activity.truncate(ACTIVITY_KEPT);
        self.bump();
    }

    fn cpty_name(&self) -> &'static str {
        self.role.name()
    }

    fn party_name(&self, p: &str) -> String {
        match p {
            WH3PL => "3PL".to_string(),
            CPTY => self.cpty_name().to_string(),
            VENDOR => "Vendor".to_string(),
            other => other.to_string(),
        }
    }

    fn names(&self, parties: &[String]) -> String {
        parties.iter().map(|p| self.party_name(p)).collect::<Vec<_>>().join(" + ")
    }

    /// Rebuilds the network: the 3PL with its three cameras, the counterparty and the (non-member)
    /// vendor, with 30 minutes of footage already recorded and anchored. The role is kept.
    pub fn reset(&mut self) -> StateView {
        if let Some(t) = self.auto_task.take() {
            t.abort();
        }
        let mut ledger: Box<dyn AnchorLedger> = Box::new(SimLedger::new());
        for p in [WH3PL, CPTY, VENDOR] {
            ledger.add_party(p);
        }
        self.start_ms = demo_start_ms();
        self.clock_ms = self.start_ms;
        let mut cams = Vec::new();
        for cfg in &CAMERAS {
            let camera = Camera::generate(cfg.id, SITE, cfg.zone);
            let node = ChainNode::new(WH3PL, cfg.id, iso_micros(self.start_ms));
            ledger
                .create_chain(&ChainSpec {
                    chain_id: cfg.id.to_string(),
                    template: TEMPLATE.to_string(),
                    profile: PROFILE.to_string(),
                    submitter: WH3PL.to_string(),
                    members: vec![WH3PL.to_string(), CPTY.to_string()],
                    genesis_head: node.genesis.clone(),
                    attributes: BTreeMap::from([
                        ("device_key".to_string(), camera.public_key()),
                        ("site".to_string(), SITE.to_string()),
                        ("zone".to_string(), cfg.zone.to_string()),
                    ]),
                })
                .expect("parties exist");
            cams.push(Cam { cfg, camera, node, footage: Footage::new(), next_seq: 1 });
        }
        self.ledger = ledger;
        self.cams = cams;
        self.vendor_member = false;
        self.incident = None;
        self.handover.clear();
        self.export = None;
        self.auto_ticks = 0;
        self.last_verify = None;
        self.activity.clear();
        // A little history so the demo opens on a live network.
        for _ in 0..SEED_MINUTES * SEGMENTS_PER_BATCH {
            self.tick(true);
        }
        self.log(
            Level::Info,
            "Network",
            format!(
                "Started: the 3PL's {} cameras and the {} on one private synchronizer. {SEED_MINUTES} minutes of \
                 footage recorded; every minute anchored and co-signed. The video never left the 3PL.",
                CAMERAS.len(),
                self.cpty_name().to_lowercase()
            ),
        );
        self.state()
    }

    /// Ten seconds pass: every camera records one segment. On a minute boundary, every node seals.
    fn tick(&mut self, quiet: bool) {
        let start = self.clock_ms;
        for c in &mut self.cams {
            let seq = c.next_seq;
            c.next_seq += 1;
            let scene = SceneScript::scene(c.cfg.zone, seq);
            let bytes = synth_bytes(c.cfg.id, seq, scene, "capture");
            let record = c.camera.record_segment(seq, start, &bytes);
            c.footage.insert(seq, Clip { scene, bytes });
            c.node.ingest(record);
        }
        self.clock_ms += SEGMENT_MS;
        if (self.clock_ms - self.start_ms) % MINUTE_MS == 0 {
            self.seal_minute(quiet);
        } else if !quiet {
            self.bump();
        }
    }

    fn seal_minute(&mut self, quiet: bool) {
        let to = iso_micros(self.clock_ms);
        let (mut roots, mut segments, mut signers, mut errors) = (Vec::new(), 0, Vec::new(), Vec::new());
        for c in &mut self.cams {
            match c.node.seal(self.ledger.as_mut(), &to) {
                Ok((batch, anchor)) => {
                    roots.push(format!("{} {}", c.cfg.id, short(&batch.merkle_root)));
                    segments += batch.event_count as usize;
                    signers = anchor.signatories;
                }
                Err(e) => errors.push(e.0),
            }
        }
        for e in errors {
            self.log(Level::Warn, "Network", e);
        }
        if !quiet {
            let from = iso_micros(self.clock_ms - MINUTE_MS);
            self.log(
                Level::Ok,
                WH3PL,
                format!(
                    "Minute {}–{} sealed: {} hashed into {} (roots {}). Anchored, co-signed by {}",
                    hm(&from),
                    hm(&to),
                    plural(segments, "segment"),
                    plural(roots.len(), "Merkle root"),
                    roots.join(", "),
                    self.names(&signers)
                ),
            );
        }
    }

    fn cam(&self, id: &str) -> Result<&Cam> {
        self.cams.iter().find(|c| c.cfg.id == id).ok_or_else(|| DemoError::not_found(format!("Unknown camera {id}")))
    }

    fn cam_mut(&mut self, id: &str) -> Result<&mut Cam> {
        self.cams
            .iter_mut()
            .find(|c| c.cfg.id == id)
            .ok_or_else(|| DemoError::not_found(format!("Unknown camera {id}")))
    }

    fn is_party(&self, p: &str) -> bool {
        [WH3PL, CPTY, VENDOR].contains(&p)
    }

    pub fn is_auto(&self) -> bool {
        self.auto_task.is_some()
    }

    fn sealed_to_ms(&self) -> i64 {
        self.clock_ms - (self.clock_ms - self.start_ms) % MINUTE_MS
    }

    pub fn state(&self) -> StateView {
        StateView {
            version: self.version,
            auto: self.is_auto(),
            clock: ClockView {
                now: iso_micros(self.clock_ms),
                start: iso_micros(self.start_ms),
                sealed_to: iso_micros(self.sealed_to_ms()),
            },
            operator: PartyView {
                id: WH3PL.into(),
                name: "3PL warehouse operator".into(),
                description: format!("Owns the cameras and the video storage at {SITE}. Submits every anchor."),
            },
            counterparty: CounterpartyView {
                id: CPTY.into(),
                name: self.cpty_name().into(),
                description: self.role.description().into(),
                role: self.role,
            },
            vendor: VendorView {
                id: VENDOR.into(),
                name: "Vendor (goods owner)".into(),
                description: if self.vendor_member {
                    "Member: holds its own ledger copy and co-signs every new anchor.".into()
                } else {
                    "Not on the ledger. Verifies the incident window from an evidence package.".into()
                },
                member: self.vendor_member,
            },
            roles: CounterpartyRole::ALL.iter().map(|r| RoleOption { role: *r, name: r.name().into() }).collect(),
            cameras: self
                .cams
                .iter()
                .map(|c| CameraView {
                    id: c.cfg.id.into(),
                    name: c.cfg.name.into(),
                    site: SITE.into(),
                    zone: c.cfg.zone.into(),
                    key_id: c.camera.info.key_id.clone(),
                    device_key: c.camera.public_key(),
                    pending: c.node.pending.len(),
                    last_seq: c.node.last_seq(),
                    last_head: c.node.last_head().to_string(),
                    stored: c.footage.len(),
                    members: self.ledger.copy(WH3PL, c.cfg.id).map(|x| x.members.clone()).unwrap_or_default(),
                })
                .collect(),
            segment_seconds: (SEGMENT_MS / 1000) as u32,
            segments_per_batch: SEGMENTS_PER_BATCH as u32,
            incident: self.incident.clone(),
            export: self.export.clone(),
            last_verify: self.last_verify.clone(),
            activity: self.activity.iter().take(ACTIVITY_SHOWN).cloned().collect(),
            envelopes: self.ledger.envelopes().len(),
        }
    }

    /// The cameras record `segments` more segments each (default one minute).
    pub fn record(&mut self, segments: Option<i64>) -> StateView {
        let n = segments.filter(|c| *c != 0).unwrap_or(SEGMENTS_PER_BATCH).clamp(1, 60);
        for _ in 0..n {
            self.tick(false);
        }
        self.state()
    }

    /// The recorder as the 3PL (or an insider) sees it right now.
    pub fn footage(&self, cam: &str) -> Result<FootageView> {
        let c = self.cam(cam)?;
        let view = |id: &str, batch_seq: Option<u64>, index: usize, r: &SegmentRecord| SegmentView {
            id: id.to_string(),
            seq: r.seq,
            batch_seq,
            index,
            start: r.start.clone(),
            end: r.end.clone(),
            scene: c.footage.get(r.seq).map(|clip| clip.scene),
            content_hash: r.content_hash.clone(),
            device_sig: r.device_sig.clone(),
        };
        let sealed = c.node.store.iter().map(|r| view(&r.id, Some(r.batch_seq), r.index, &r.record));
        let pending = c.node.pending.iter().enumerate().map(|(i, p)| view(&p.id, None, i, &p.record));
        Ok(FootageView { cam: cam.into(), segments: sealed.chain(pending).collect() })
    }

    pub fn batch_detail(&self, cam: &str, seq: u64) -> Result<BatchDetail> {
        let node = &self.cam(cam)?.node;
        let batch = node.batch(seq).ok_or_else(|| DemoError::not_found(format!("{cam} has no batch #{seq}")))?;
        let anchor = self
            .ledger
            .copy(WH3PL, cam)
            .and_then(|c| c.anchors.iter().find(|a| a.seq == seq))
            .ok_or_else(|| DemoError::not_found(format!("The 3PL's ledger copy has no anchor #{seq} for {cam}")))?;
        Ok(BatchDetail {
            cam: cam.into(),
            seq,
            total_batches: node.batches.len(),
            batch: batch.into(),
            anchor: anchor.clone(),
        })
    }

    pub fn ledger_view(&self, party: &str) -> Result<LedgerView> {
        if !self.is_party(party) {
            return Err(DemoError::not_found(format!("Unknown party {party}")));
        }
        let env = self.ledger.envelopes();
        Ok(LedgerView {
            party: party.into(),
            chains: self.ledger.view(party),
            synchronizer: env.iter().rev().take(SYNC_SHOWN).cloned().collect(),
            total_envelopes: env.len(),
        })
    }

    /// `actor` tries to archive an anchor on its own. The ledger refuses: every signatory must sign.
    pub fn rewrite(&mut self, actor: &str, chain: &str, seq: i64) -> Result<ArchiveAttempt> {
        if !self.is_party(actor) {
            return Err(DemoError::bad(format!("Unknown party {actor}")));
        }
        self.cam(chain)?;
        let seq = seq.max(0) as u64;
        let res = self.ledger.attempt_archive(chain, actor, seq).map_err(|e| DemoError::bad(e.0))?;
        let who = self.party_name(actor);
        let text = if res.required.is_empty() {
            format!("{who} tried to archive an anchor of {chain}. Rejected: it is not a stakeholder of that chain")
        } else {
            format!(
                "{who} tried to archive anchor #{seq} of {chain}. Rejected: missing signature from {}",
                self.names(&res.missing)
            )
        };
        self.log(Level::Warn, actor, text);
        Ok(res)
    }

    /// An insider alters the recorder after the footage was anchored.
    pub fn tamper(&mut self, req: &TamperRequest) -> Result<TamperResult> {
        let c = self.cam_mut(&req.cam)?;
        if c.node.pending.iter().any(|p| p.record.seq == req.seq) {
            return Err(DemoError::bad(format!(
                "Segment #{} is not anchored yet. Record until the minute is sealed.",
                req.seq
            )));
        }
        let (footage, rows, camera) = (&mut c.footage, &mut c.node.store, &c.camera);
        let description = match req.mode {
            TamperMode::Modify => tamper::modify(footage, rows, camera, req.seq, req.cover),
            TamperMode::Replay => tamper::replay(footage, rows, camera, req.seq, req.cover),
            TamperMode::Delete => tamper::delete_segment(footage, rows, camera, req.seq, req.cover),
            TamperMode::DeleteMinute => tamper::delete_minute(footage, rows, req.seq),
        }
        .map_err(|e| DemoError::not_found(e.0))?;
        self.log(
            Level::Danger,
            WH3PL,
            format!(
                "Someone with access to the 3PL's recorder altered {} after it was anchored: {description}",
                req.cam
            ),
        );
        Ok(TamperResult { ok: true, description })
    }

    /// Raises the incident on the consignment and fixes the window everyone will verify.
    pub fn raise_incident(&mut self, req: &IncidentRequest) -> Result<IncidentView> {
        let parse = |t: &Option<String>, default: i64| match t {
            Some(t) => parse_ms(t).ok_or_else(|| DemoError::bad(format!("Not a timestamp: {t}"))),
            None => Ok(default),
        };
        let from = parse(&req.from, self.start_ms)?;
        let to = parse(&req.to, self.start_ms + SEED_MINUTES * MINUTE_MS)?;
        if (from - self.start_ms) % MINUTE_MS != 0 || (to - self.start_ms) % MINUTE_MS != 0 {
            return Err(DemoError::bad("The window must start and end on a full minute"));
        }
        if from < self.start_ms || to <= from {
            return Err(DemoError::bad("The window must start after recording began and end after it starts"));
        }
        if to > self.sealed_to_ms() {
            return Err(DemoError::bad("The window ends after the last anchored minute"));
        }
        let cams: Vec<String> =
            if req.cams.is_empty() { CAMERAS.iter().map(|c| c.id.to_string()).collect() } else { req.cams.clone() };
        for c in &cams {
            self.cam(c)?;
        }
        self.incidents_raised += 1;
        let incident = IncidentView {
            id: format!("INC-{:04}", self.incidents_raised),
            consignment: CONSIGNMENT.into(),
            from: iso_micros(from),
            to: iso_micros(to),
            cams,
        };
        self.incident = Some(incident.clone());
        self.handover.clear();
        self.export = None;
        self.last_verify = None;
        self.log(
            Level::Warn,
            VENDOR,
            format!(
                "Shortage reported on consignment {CONSIGNMENT}. Incident {} raised: footage from {} to {} on {} is now in question",
                incident.id,
                hm(&incident.from),
                hm(&incident.to),
                incident.cams.join(", ")
            ),
        );
        Ok(incident)
    }

    fn incident(&self) -> Result<IncidentView> {
        self.incident.clone().ok_or_else(|| DemoError::bad("Raise the incident first"))
    }

    /// The 3PL builds the evidence package for the incident window and hands it to the vendor:
    /// records, salts, inclusion proofs and anchors, plus the clips of that window only.
    pub fn export(&mut self) -> Result<ExportView> {
        let view = self.export_inner()?;
        self.log(
            Level::Info,
            WH3PL,
            format!(
                "Exported the evidence package for {}: {} across {} with their inclusion proofs ({} KB of hashes and signatures)",
                view.incident,
                plural(view.segments, "segment record"),
                plural(view.batches, "anchored minute"),
                view.package_bytes.div_ceil(1024)
            ),
        );
        Ok(view)
    }

    fn export_inner(&mut self) -> Result<ExportView> {
        let inc = self.incident()?;
        let mut handover = Vec::new();
        let (mut batches, mut segments, mut anchors, mut bytes) = (0, 0, 0, 0);
        for id in &inc.cams {
            let c = self.cam(id)?;
            // The exporter reads the anchors from its own ledger copy.
            let copy = self.ledger.copy(WH3PL, id).ok_or_else(|| DemoError::bad(format!("No chain for {id}")))?;
            let (from_seq, to_seq) = window_seqs(&copy.anchors, &inc)
                .ok_or_else(|| DemoError::bad(format!("No anchored minute of {id} falls in the window")))?;
            let leaves = |seq: u64| c.node.batch(seq).map(Batch::leaves);
            let package =
                build_package(id, &copy.genesis_head, &copy.anchors, &c.node.store, &leaves, from_seq, to_seq);
            let clips = c.footage.excerpt(package.items.iter().map(|i| i.record.seq));
            batches += (to_seq - from_seq + 1) as usize;
            segments += package.items.len();
            anchors += package.anchors.len();
            bytes += serde_json::to_vec(&package).map(|v| v.len()).unwrap_or(0);
            handover.push(Handover { cam: id.clone(), package, clips });
        }
        let view = ExportView {
            at: iso_micros(self.clock_ms),
            incident: inc.id,
            cams: inc.cams,
            batches,
            segments,
            anchors,
            package_bytes: bytes,
        };
        self.handover = handover;
        self.export = Some(view.clone());
        Ok(view)
    }

    /// Verifies the incident window.
    ///
    /// - `CPTY` is a member: it checks the 3PL's storage against the anchors on ITS OWN ledger copy.
    /// - `VENDOR` checks the evidence package. The anchors in it are compared with a co-signer's
    ///   ledger copy (its own, once it is a member), so it never takes the 3PL's word for them.
    pub fn verify(&mut self, verifier: &str) -> Result<VerifyResponse> {
        let inc = self.incident()?;
        let (method, anchors_from) = match verifier {
            CPTY => (VerifyMethod::LedgerCopy, CPTY),
            VENDOR => {
                self.export_inner()?; // the 3PL hands over its storage as it is now
                (VerifyMethod::EvidencePackage, if self.vendor_member { VENDOR } else { CPTY })
            }
            _ => return Err(DemoError::bad("Verifier must be CPTY or VENDOR")),
        };

        let mut cams = Vec::new();
        for id in &inc.cams {
            let c = self.cam(id)?;
            let copy = self
                .ledger
                .copy(anchors_from, id)
                .ok_or_else(|| DemoError::bad(format!("{anchors_from}'s node holds no copy of chain {id}")))?;
            let device_key = copy.attributes.get("device_key").map(String::as_str).unwrap_or_default();
            let scene = |seq: u64| c.footage.get(seq).map(|clip| clip.scene);
            let input = match method {
                VerifyMethod::LedgerCopy => {
                    // The member's verifier asks the 3PL's node for the window's records and proofs,
                    // and reads the stored video where it lives. Nothing but hashes is compared.
                    let window = window_seqs(&copy.anchors, &inc);
                    let (from_seq, to_seq) = window.unwrap_or((1, 0));
                    let leaves = |seq: u64| c.node.batch(seq).map(Batch::leaves);
                    let package =
                        build_package(id, &copy.genesis_head, &copy.anchors, &c.node.store, &leaves, from_seq, to_seq);
                    let checks = check_segments(package.items.iter().map(|i| &i.record), &c.footage, device_key);
                    let trail = verify_trail(id, &copy.genesis_head, &copy.anchors, &c.node.store);
                    CamInput { report: verify_package(&package, &copy.anchors), package, checks, trail: Some(trail) }
                }
                VerifyMethod::EvidencePackage => {
                    let h = self
                        .handover
                        .iter()
                        .find(|h| h.cam == *id)
                        .ok_or_else(|| DemoError::bad(format!("No evidence package for {id}")))?;
                    let checks = check_segments(h.package.items.iter().map(|i| &i.record), &h.clips, device_key);
                    CamInput {
                        report: verify_package(&h.package, &copy.anchors),
                        package: h.package.clone(),
                        checks,
                        trail: None,
                    }
                }
            };
            cams.push(cam_verdict(id, &inc, &copy.anchors, &input, &scene));
        }

        let ok = cams.iter().all(|c| c.ok);
        let segments_checked: usize = cams.iter().map(|c| c.segments.len()).sum();
        let span = format!("from {} to {}", hm(&inc.from), hm(&inc.to));
        let verdict = if ok {
            format!(
                "Footage {span} on {} is genuine and complete: {} match the anchors co-signed at capture.",
                if cams.len() == 1 { cams[0].cam.clone() } else { format!("all {} cameras", cams.len()) },
                plural(segments_checked, "segment")
            )
        } else {
            let bad: Vec<String> =
                cams.iter().filter(|c| !c.ok).map(|c| format!("{}: {}", c.cam, problems(c))).collect();
            format!("Footage {span} cannot be relied on. {}.", bad.join("; "))
        };
        let resp = VerifyResponse {
            verifier: verifier.into(),
            verifier_name: self.party_name(verifier),
            method,
            anchors_from: anchors_from.into(),
            incident: inc,
            ok,
            verdict,
            segments_checked,
            at: iso_micros(self.clock_ms),
            cams,
        };
        self.last_verify =
            Some(LastVerify { verifier: verifier.into(), ok, verdict: resp.verdict.clone(), at: resp.at.clone() });
        let how = match method {
            VerifyMethod::LedgerCopy => "against its own ledger copy".to_string(),
            VerifyMethod::EvidencePackage if anchors_from == VENDOR => {
                "from the evidence package, against its own ledger copy".to_string()
            }
            VerifyMethod::EvidencePackage => {
                format!("from the evidence package, with anchors confirmed by the {}", self.cpty_name().to_lowercase())
            }
        };
        self.log(
            if ok { Level::Ok } else { Level::Danger },
            verifier,
            format!("{} verified {how}. {}", self.party_name(verifier), resp.verdict),
        );
        Ok(resp)
    }

    /// Changes who the counterparty is. Signatories are fixed when a chain is created, so the
    /// network is rebuilt.
    pub fn set_role(&mut self, role: CounterpartyRole) -> StateView {
        self.role = role;
        self.reset()
    }

    /// Optional: the vendor becomes a member. Its node receives the history and co-signs from now on.
    pub fn add_vendor(&mut self) -> Result<StateView> {
        if self.vendor_member {
            return Ok(self.state());
        }
        for cfg in &CAMERAS {
            self.ledger.add_member(cfg.id, VENDOR, WH3PL).map_err(|e| DemoError::bad(e.0))?;
        }
        self.vendor_member = true;
        self.log(
            Level::Ok,
            VENDOR,
            format!(
                "Joined the network. The 3PL and the {} signed its admission. Its node now holds a copy of every \
                 camera chain, and every new anchor is co-signed by all three",
                self.cpty_name().to_lowercase()
            ),
        );
        Ok(self.state())
    }

    /// One auto-record step: every camera records a segment. Returns `false` once auto-record
    /// has run long enough and should stop.
    pub fn auto_tick(&mut self) -> bool {
        self.tick(false);
        self.auto_ticks += 1;
        self.auto_ticks < AUTO_MAX_TICKS
    }
}

/// The batches of a chain whose minute lies inside the incident window, as `(first, last)`.
fn window_seqs(anchors: &[Anchor], inc: &IncidentView) -> Option<(u64, u64)> {
    let mut inside = anchors
        .iter()
        .filter(|a| a.from_time.as_str() >= inc.from.as_str() && a.to_time.as_str() <= inc.to.as_str())
        .map(|a| a.seq);
    let first = inside.next()?;
    Some((first, inside.next_back().unwrap_or(first)))
}

/// Everything computed about one camera before it is turned into a verdict.
struct CamInput {
    package: EvidencePackage<SegmentRecord>,
    report: PackageReport,
    checks: Vec<SegmentCheck>,
    /// Whole-chain recomputation; only a member with the full trail in reach has it.
    trail: Option<VerifyReport>,
}

fn problems(c: &CamVerdict) -> String {
    let n = &c.counts;
    let mut parts = Vec::new();
    for (count, what) in
        [(n.modified, "modified"), (n.replayed, "looped"), (n.missing, "missing"), (n.bad_signature, "badly signed")]
    {
        if count > 0 {
            parts.push(format!("{} {what}", plural(count, "segment")));
        }
    }
    match (&c.issue, parts.is_empty()) {
        (Some(issue), true) => issue.clone(),
        (Some(issue), false) => format!("{}; {issue}", parts.join(", ")),
        (None, _) => parts.join(", "),
    }
}

/// Turns the generic package report and the profile checks into per-slot verdicts. The slots come
/// from `trusted` anchors alone: each anchored minute says how many segments must exist and which.
fn cam_verdict(
    cam: &str,
    inc: &IncidentView,
    trusted: &[Anchor],
    input: &CamInput,
    scene: &dyn Fn(u64) -> Option<Scene>,
) -> CamVerdict {
    let CamInput { package, report, checks, trail } = input;
    let window = window_seqs(trusted, inc);
    let mut issue = report.issue.clone();
    if window != Some((package.from_seq, package.to_seq)) && issue.is_none() {
        issue = Some("the records handed over do not cover the incident window".into());
    }
    let by_seq: HashMap<u64, &SegmentCheck> = checks.iter().map(|c| (c.seq, c)).collect();
    let chain_start = trusted.first().and_then(|a| parse_ms(&a.from_time)).unwrap_or_default();
    let when = |seq: u64| iso_micros(chain_start + (seq as i64 - 1) * SEGMENT_MS);

    let (mut segments, mut batches, mut counts) = (Vec::new(), Vec::new(), StatusCounts::default());
    let (from_seq, to_seq) = window.unwrap_or((1, 0));
    for a in trusted.iter().filter(|a| a.seq >= from_seq && a.seq <= to_seq) {
        let pb = report.batches.iter().find(|b| b.seq == a.seq);
        let first_seq: u64 = a.from_pos.parse().unwrap_or_default();
        let from_ms = parse_ms(&a.from_time).unwrap_or_default();
        let mut items: Vec<&PackagedRecord<SegmentRecord>> =
            package.items.iter().filter(|i| i.batch_seq == a.seq).collect();
        items.sort_by_key(|i| i.index);

        let mut statuses: BTreeMap<usize, SegmentStatus> = BTreeMap::new();
        for index in 0..a.event_count as usize {
            let start = from_ms + index as i64 * SEGMENT_MS;
            let item = items.iter().find(|i| i.index == index);
            let included = pb.and_then(|b| b.items.iter().find(|i| i.index == index)).is_some_and(|i| i.included);
            let check = item.and_then(|i| by_seq.get(&i.record.seq));
            let (status, note) = match (item, check) {
                (None, _) | (_, None) => (
                    SegmentStatus::Missing,
                    Some(
                        "The anchor proves a segment was recorded in this slot. It is no longer in storage."
                            .to_string(),
                    ),
                ),
                (Some(_), Some(c)) => match c.status {
                    SegmentStatus::Missing => (
                        SegmentStatus::Missing,
                        Some("The record exists but its video is gone from the recorder.".to_string()),
                    ),
                    SegmentStatus::Replayed => {
                        let of = c.replay_of.unwrap_or_default();
                        (
                            SegmentStatus::Replayed,
                            Some(format!(
                                "The video is identical to the segment at {} (#{of}): earlier footage was looped in.",
                                hms(&when(of))
                            )),
                        )
                    }
                    _ if !included => (
                        SegmentStatus::Modified,
                        Some(if c.signature_ok && c.content_ok {
                            "The record is signed with the camera's key and matches the video, but it is not the \
                             record anchored at capture: it does not hash to the Merkle root the counterparty co-signed."
                                .to_string()
                        } else {
                            "The record does not hash to the leaf anchored at capture.".to_string()
                        }),
                    ),
                    SegmentStatus::BadSignature => (
                        SegmentStatus::BadSignature,
                        Some("The record is not signed by the camera key registered at genesis.".to_string()),
                    ),
                    SegmentStatus::Modified => (
                        SegmentStatus::Modified,
                        Some(
                            "The video in storage no longer hashes to the content hash the camera signed.".to_string(),
                        ),
                    ),
                    SegmentStatus::Ok => (SegmentStatus::Ok, None),
                },
            };
            match status {
                SegmentStatus::Ok => counts.ok += 1,
                SegmentStatus::Modified => counts.modified += 1,
                SegmentStatus::Missing => counts.missing += 1,
                SegmentStatus::Replayed => counts.replayed += 1,
                SegmentStatus::BadSignature => counts.bad_signature += 1,
            }
            statuses.insert(index, status);
            segments.push(SegmentVerdict {
                seq: first_seq + index as u64,
                batch_seq: a.seq,
                index,
                start: iso_micros(start),
                end: iso_micros(start + SEGMENT_MS),
                status,
                note,
                scene: item.and_then(|i| scene(i.record.seq)),
                recorded_hash: item.map(|i| i.record.content_hash.clone()),
                observed_hash: check.and_then(|c| c.observed_hash.clone()),
            });
        }

        // Batch level: a member recomputes root and head from the whole trail; a package verifier
        // recomputes the root from the records present and relies on the checked anchor chain.
        let t = trail.as_ref().and_then(|t| t.batches.iter().find(|b| b.seq == a.seq));
        let (root_match, head_match, count_match, recomputed_root, tree) = match (t, pb) {
            (Some(t), _) => (t.root_match, t.head_match, t.count_match, t.recomputed.root.clone(), t.tree.clone()),
            (None, Some(b)) => (
                b.root_match,
                report.chain_ok && report.anchors_trusted,
                b.count_match,
                b.recomputed_root.clone(),
                b.tree.clone(),
            ),
            (None, None) => (false, false, false, String::new(), sthira_core::merkle_tree(&[])),
        };
        let segments_ok = statuses.values().all(|s| *s == SegmentStatus::Ok);
        batches.push(BatchVerdict {
            seq: a.seq,
            ok: root_match && count_match && segments_ok,
            from_time: a.from_time.clone(),
            to_time: a.to_time.clone(),
            root_match,
            head_match,
            count_match,
            expected_count: a.event_count,
            present_count: items.len() as u64,
            anchored_root: a.merkle_root.clone(),
            recomputed_root,
            signatories: a.signatories.clone(),
            contract_id: a.contract_id.clone(),
            tree,
            bad_leaves: items
                .iter()
                .enumerate()
                .filter(|(_, i)| statuses.get(&i.index).is_some_and(|s| *s != SegmentStatus::Ok))
                .map(|(pos, _)| pos)
                .collect(),
            leaf_labels: items.iter().map(|i| format!("#{}", i.record.seq)).collect(),
        });
    }

    let ok = issue.is_none() && window.is_some() && batches.iter().all(|b| b.ok);
    let mut v = CamVerdict { cam: cam.into(), ok, verdict: String::new(), issue, counts, segments, batches };
    let span = format!("Footage from {} to {} on {cam}", hm(&inc.from), hm(&inc.to));
    v.verdict = if ok {
        format!("{span} is genuine and complete")
    } else {
        format!("{span} is not trustworthy: {}", problems(&v))
    };
    v
}

/// Turns auto-record on or off. The background task holds only a weak reference to the world.
pub fn set_auto(shared: &Shared, on: bool) -> StateView {
    let mut w = shared.lock().expect("world lock");
    if on && !w.is_auto() {
        let weak: Weak<Mutex<World>> = Arc::downgrade(shared);
        w.auto_ticks = 0;
        w.auto_task = Some(tokio::spawn(async move {
            let mut tick = tokio::time::interval_at(tokio::time::Instant::now() + AUTO_INTERVAL, AUTO_INTERVAL);
            loop {
                tick.tick().await;
                let Some(world) = weak.upgrade() else { break };
                let mut w = world.lock().expect("world lock");
                if !w.auto_tick() {
                    w.auto_task = None;
                    w.log(Level::Info, "Network", "Auto-record off after an hour of demo time");
                    break;
                }
            }
        }));
        w.log(
            Level::Info,
            "Network",
            "Auto-record on: every camera records a 10-second segment each second (×10); every minute is sealed and anchored",
        );
    } else if !on && w.is_auto() {
        if let Some(t) = w.auto_task.take() {
            t.abort();
        }
        w.log(Level::Info, "Network", "Auto-record off");
    }
    w.state()
}
