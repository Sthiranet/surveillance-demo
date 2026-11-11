//! Runs the narrated story (scenes 1–8 of the build plan) against the backend and checks each claim.

use sthira_profile_surveillance::SegmentStatus;
use surveillance_director::views::{
    CamVerdict, CounterpartyRole, IncidentRequest, SegmentVerdict, TamperMode, TamperRequest, VerifyMethod,
    VerifyResponse,
};
use surveillance_director::world::{CPTY, VENDOR, WH3PL};
use surveillance_director::{World, set_auto};

const CAGE: &str = "CAM-CAGE";
const DOCK: &str = "CAM-DOCK";

fn tamper(w: &mut World, cam: &str, seq: u64, mode: TamperMode, cover: bool) -> String {
    w.tamper(&TamperRequest { cam: cam.into(), seq, mode, cover }).expect("tamper").description
}

fn cam<'a>(v: &'a VerifyResponse, id: &str) -> &'a CamVerdict {
    v.cams.iter().find(|c| c.cam == id).expect("camera in verdict")
}

fn seg(c: &CamVerdict, seq: u64) -> &SegmentVerdict {
    c.segments.iter().find(|s| s.seq == seq).expect("slot in verdict")
}

fn bad_slots(c: &CamVerdict) -> Vec<(u64, SegmentStatus)> {
    c.segments.iter().filter(|s| s.status != SegmentStatus::Ok).map(|s| (s.seq, s.status)).collect()
}

/// A fresh network with the incident already raised on the default window (08:20–08:30, all cameras).
fn with_incident() -> World {
    let mut w = World::new();
    w.raise_incident(&IncidentRequest::default()).unwrap();
    w
}

#[tokio::test]
async fn the_story() {
    let mut w = World::new();

    // ── Scene 1 · Live network: 30 minutes anchored per camera; a new minute seals and is co-signed. ──
    let s = w.state();
    assert_eq!(s.cameras.len(), 3);
    assert!(s.cameras.iter().all(|c| c.last_seq == 30 && c.stored == 180 && c.pending == 0));
    assert_eq!(s.cameras[0].members, [WH3PL, CPTY]);
    assert_eq!((&s.clock.start[11..19], &s.clock.now[11..19]), ("08:00:00", "08:30:00"));

    let s = w.record(Some(2));
    assert!(s.cameras.iter().all(|c| c.pending == 2 && c.last_seq == 30), "the minute fills segment by segment");
    assert_eq!(w.tamper(&TamperRequest { cam: CAGE.into(), seq: 181, ..Default::default() }).unwrap_err().status, 400);
    let s = w.record(Some(4));
    assert!(s.cameras.iter().all(|c| c.pending == 0 && c.last_seq == 31));

    let b = w.batch_detail(CAGE, 31).unwrap();
    assert_eq!(b.batch.event_count, 6);
    assert_eq!(b.anchor.signatories, [WH3PL, CPTY]);
    assert_eq!(b.anchor.template, "Sthiranet.Surveillance:SegmentAnchor");
    assert_eq!(b.anchor.merkle_root, b.batch.merkle_root);
    assert_eq!(b.anchor.prev_head, w.batch_detail(CAGE, 30).unwrap().batch.head);
    assert_eq!((b.anchor.from_pos.as_str(), b.anchor.to_pos.as_str()), ("0000000181", "0000000186"));
    assert_eq!((&b.anchor.from_time[11..19], &b.anchor.to_time[11..19]), ("08:30:00", "08:31:00"));
    assert_eq!(b.batch.items[0].salt.len(), 64, "32-byte salts");

    // The counterparty holds its own copy of all three chains, with each camera's key agreed at
    // genesis. The vendor is not on the ledger at all.
    let cp = w.ledger_view(CPTY).unwrap();
    assert_eq!(cp.chains.len(), 3);
    let cage = cp.chains.iter().find(|c| c.chain_id == CAGE).unwrap();
    assert_eq!((cage.last_seq, cage.profile.as_str(), cage.submitter.as_str()), (31, "surveillance/v1", WH3PL));
    assert_eq!(cage.attributes["device_key"], w.state().cameras[1].device_key);
    assert!(w.ledger_view(VENDOR).unwrap().chains.is_empty());

    // ── Scene 2 · Incident: the window is fixed before anyone verifies. ──
    assert_eq!(w.verify(CPTY).unwrap_err().status, 400, "nothing to verify before the incident");
    let inc = w.raise_incident(&IncidentRequest::default()).unwrap();
    assert_eq!(
        (inc.consignment.as_str(), &inc.from[11..16], &inc.to[11..16], inc.cams.len()),
        ("C-1047", "08:20", "08:30", 3)
    );

    // ── Scene 3 · Clean verification: by the counterparty on its own ledger copy… ──
    let v = w.verify(CPTY).unwrap();
    assert!(v.ok, "{}", v.verdict);
    assert_eq!((v.method, v.anchors_from.as_str(), v.segments_checked), (VerifyMethod::LedgerCopy, CPTY, 180));
    assert!(v.verdict.contains("genuine and complete"));
    assert!(v.cams.iter().all(|c| c.batches.len() == 10 && c.counts.ok == 60));
    assert!(cam(&v, CAGE).batches.iter().all(|b| b.root_match && b.head_match && b.count_match));

    // …and by the vendor, who has no node: from the evidence package alone.
    let x = w.export().unwrap();
    assert_eq!((x.segments, x.batches), (180, 30));
    let v = w.verify(VENDOR).unwrap();
    assert!(v.ok, "{}", v.verdict);
    assert_eq!((v.method, v.anchors_from.as_str()), (VerifyMethod::EvidencePackage, CPTY));
    assert!(w.state().last_verify.unwrap().ok);

    // ── Scene 4 · Modification: a spliced segment is pinpointed; the rest of the footage stands. ──
    let d = tamper(&mut w, CAGE, 128, TamperMode::Modify, false);
    assert!(d.contains("08:21:10–08:21:20"), "{d}");
    for verifier in [CPTY, VENDOR] {
        let v = w.verify(verifier).unwrap();
        assert!(!v.ok && v.verdict.contains("cannot be relied on"), "{}", v.verdict);
        let c = cam(&v, CAGE);
        assert_eq!(bad_slots(c), [(128, SegmentStatus::Modified)]);
        assert_eq!(c.verdict, "Footage from 08:20 to 08:30 on CAM-CAGE is not trustworthy: 1 segment modified");
        let s = seg(c, 128);
        assert_eq!(&s.start[11..19], "08:21:10");
        assert_ne!(s.recorded_hash, s.observed_hash);
        let b = c.batches.iter().find(|b| b.seq == 22).unwrap();
        assert!(!b.ok && b.root_match, "the record is still the anchored one; the video is not");
        assert_eq!(b.bad_leaves, [1]);
        assert!(cam(&v, DOCK).ok && cam(&v, "CAM-DISPATCH").ok);
    }

    // A loop of earlier footage is recognised as a copy, and which footage it copies.
    tamper(&mut w, DOCK, 150, TamperMode::Replay, false);
    for verifier in [CPTY, VENDOR] {
        let v = w.verify(verifier).unwrap();
        let s = seg(cam(&v, DOCK), 150);
        assert_eq!(s.status, SegmentStatus::Replayed);
        assert!(s.note.as_ref().unwrap().contains("08:21:50"), "{:?}", s.note);
    }

    // ── Scene 5 · Deletion: the anchor says six segments existed in that minute. ──
    let mut w = with_incident();
    tamper(&mut w, CAGE, 128, TamperMode::Delete, false);
    for verifier in [CPTY, VENDOR] {
        let v = w.verify(verifier).unwrap();
        let c = cam(&v, CAGE);
        assert_eq!(bad_slots(c), [(128, SegmentStatus::Missing)], "the five that remain are still proven");
        let b = c.batches.iter().find(|b| b.seq == 22).unwrap();
        assert!(!b.count_match && !b.root_match);
        assert_eq!((b.expected_count, b.present_count), (6, 5));
        assert!(seg(c, 128).scene.is_none());
    }
    // A member recomputing the whole trail also sees every later head move.
    let v = w.verify(CPTY).unwrap();
    let later = cam(&v, CAGE).batches.iter().find(|b| b.seq == 23).unwrap();
    assert!(later.root_match && !later.head_match && later.ok);

    // A whole minute: an anchored batch with nothing left in it.
    let d = tamper(&mut w, CAGE, 157, TamperMode::DeleteMinute, false);
    assert!(d.contains("08:26:00–08:27:00"), "{d}");
    let v = w.verify(VENDOR).unwrap();
    let c = cam(&v, CAGE);
    assert_eq!((c.counts.missing, c.counts.ok), (7, 53));
    let b = c.batches.iter().find(|b| b.seq == 27).unwrap();
    assert_eq!((b.expected_count, b.present_count), (6, 0));
    assert!(c.verdict.ends_with("7 segments missing"), "{}", c.verdict);

    // ── Scene 6 · Covering tracks: re-hashed and re-signed with the camera's own key. ──
    let mut w = with_incident();
    let d = tamper(&mut w, CAGE, 128, TamperMode::Modify, true);
    assert!(d.contains("re-signed"), "{d}");
    for verifier in [CPTY, VENDOR] {
        let v = w.verify(verifier).unwrap();
        let c = cam(&v, CAGE);
        assert_eq!(bad_slots(c), [(128, SegmentStatus::Modified)]);
        let s = seg(c, 128);
        assert_eq!(s.recorded_hash, s.observed_hash, "hash and signature now agree with the video");
        assert!(s.note.as_ref().unwrap().contains("co-signed"), "{:?}", s.note);
        assert!(!c.batches.iter().find(|b| b.seq == 22).unwrap().root_match, "but the co-signed root does not move");
    }
    // Renumbering around a deleted segment: every later segment of the minute stops matching.
    tamper(&mut w, DOCK, 122, TamperMode::Delete, true);
    let v = w.verify(CPTY).unwrap();
    let slots = bad_slots(cam(&v, DOCK));
    assert_eq!(slots.iter().map(|s| s.0).collect::<Vec<_>>(), [122, 123, 124, 125, 126]);
    assert_eq!(slots[4].1, SegmentStatus::Missing);
    assert!(slots[..4].iter().all(|s| s.1 == SegmentStatus::Modified));

    // ── Scene 7 · Rewriting the ledger: nobody can do it alone. ──
    let r = w.rewrite(WH3PL, CAGE, 22).unwrap();
    assert!(!r.ok);
    assert_eq!(r.missing, [CPTY]);
    assert_eq!(w.rewrite(CPTY, CAGE, 22).unwrap().missing, [WH3PL]);
    assert!(w.rewrite(VENDOR, CAGE, 22).unwrap().required.is_empty(), "a non-member cannot even see the anchors");
    assert_eq!(w.ledger_view(CPTY).unwrap().chains[0].anchors.len(), 30, "nothing was archived");

    // ── Scene 8 · Outcome: the state carries one verdict; a reset restores a clean network. ──
    let last = w.state().last_verify.unwrap();
    assert!(!last.ok && last.verdict.contains("cannot be relied on"));
    w.reset();
    assert!(w.state().incident.is_none() && w.state().last_verify.is_none());
    w.raise_incident(&IncidentRequest::default()).unwrap();
    assert!(w.verify(CPTY).unwrap().ok);
}

#[test]
fn a_narrower_window_on_one_camera() {
    let mut w = World::new();
    let start = w.state().clock.start;
    let at = |hm: &str| start.replace("08:00", hm);
    let inc = w
        .raise_incident(&IncidentRequest { from: Some(at("08:20")), to: Some(at("08:25")), cams: vec![CAGE.into()] })
        .unwrap();
    assert_eq!(inc.id, "INC-0001");
    tamper(&mut w, CAGE, 128, TamperMode::Delete, false);
    tamper(&mut w, CAGE, 10, TamperMode::Modify, false); // outside the window
    for verifier in [CPTY, VENDOR] {
        let v = w.verify(verifier).unwrap();
        assert_eq!((v.cams.len(), v.segments_checked), (1, 30));
        assert_eq!(bad_slots(&v.cams[0]), [(128, SegmentStatus::Missing)]);
        assert_eq!(v.cams[0].batches.iter().map(|b| b.seq).collect::<Vec<_>>(), [21, 22, 23, 24, 25]);
    }
    assert_eq!(w.export().unwrap().segments, 29);

    // Windows that are not whole anchored minutes are refused.
    for (from, to) in [("08:20:30", "08:25:00"), ("08:25:00", "08:20:00"), ("08:20:00", "09:40:00")] {
        let (from, to) = (start.replace("08:00:00", from), start.replace("08:00:00", to));
        let req = IncidentRequest { from: Some(from), to: Some(to), cams: vec![] };
        assert_eq!(w.raise_incident(&req).unwrap_err().status, 400);
    }
    let req = IncidentRequest { cams: vec!["CAM-ROOF".into()], ..Default::default() };
    assert_eq!(w.raise_incident(&req).unwrap_err().status, 404);
}

#[test]
fn the_vendor_can_be_promoted_to_member() {
    let mut w = with_incident();
    let s = w.add_vendor().unwrap();
    assert!(s.vendor.member);
    assert_eq!(w.ledger_view(VENDOR).unwrap().chains.len(), 3, "it received the history");
    w.record(None);
    assert_eq!(w.batch_detail(DOCK, 31).unwrap().anchor.signatories, [WH3PL, CPTY, VENDOR]);
    // It now checks packages against its own ledger copy, and its signature is needed to archive.
    let v = w.verify(VENDOR).unwrap();
    assert!(v.ok && v.anchors_from == VENDOR);
    assert_eq!(w.rewrite(WH3PL, DOCK, 31).unwrap().missing, [CPTY, VENDOR]);
    assert!(w.add_vendor().unwrap().vendor.member, "idempotent");
}

#[test]
fn the_counterparty_role_is_selectable_and_resets_the_network() {
    let mut w = with_incident();
    assert_eq!(w.state().counterparty.name, "Insurer");
    let old_key = w.state().cameras[0].device_key.clone();
    let s = w.set_role(CounterpartyRole::CameraProvider);
    assert_eq!((s.counterparty.id.as_str(), s.counterparty.name.as_str()), (CPTY, "Camera provider"));
    assert!(s.incident.is_none() && s.cameras.iter().all(|c| c.last_seq == 30));
    assert_ne!(s.cameras[0].device_key, old_key, "new chains, new genesis");
    assert_eq!(w.reset().counterparty.role, CounterpartyRole::CameraProvider, "a reset keeps the role");
}

#[test]
fn errors_are_reported_with_status() {
    let mut w = World::new();
    assert_eq!(w.footage("CAM-ROOF").unwrap_err().status, 404);
    assert_eq!(w.batch_detail(CAGE, 99).unwrap_err().status, 404);
    assert_eq!(w.ledger_view("NOBODY").unwrap_err().status, 404);
    assert_eq!(w.rewrite(WH3PL, CAGE, 99).unwrap_err().status, 400);
    assert_eq!(w.export().unwrap_err().status, 400);
    w.raise_incident(&IncidentRequest::default()).unwrap();
    assert_eq!(w.verify(WH3PL).unwrap_err().status, 400);
    assert_eq!(
        w.tamper(&TamperRequest { cam: CAGE.into(), seq: 9_999, ..Default::default() }).unwrap_err().status,
        404
    );
}

#[tokio::test]
async fn auto_record_toggles_and_seals_on_the_minute() {
    let shared = World::shared();
    assert!(set_auto(&shared, true).auto);
    assert!(!set_auto(&shared, false).auto);
    let mut w = shared.lock().unwrap();
    for _ in 0..6 {
        assert!(w.auto_tick());
    }
    let s = w.state();
    assert!(s.cameras.iter().all(|c| c.last_seq == 31 && c.pending == 0));
    assert_eq!(&s.clock.now[11..19], "08:31:00");
    assert_eq!(w.footage(CAGE).unwrap().segments.len(), 186);
}
