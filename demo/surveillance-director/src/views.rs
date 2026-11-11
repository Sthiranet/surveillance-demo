//! JSON shapes the UI reads (camelCase). These are the contract the TypeScript client is generated from.

use serde::{Deserialize, Serialize};
use sthira_core::MerkleTree;
use sthira_ledger::{Anchor, ChainCopy, Envelope, Party};
use sthira_node::Batch;
use sthira_profile_surveillance::{Scene, SegmentStatus};
use utoipa::ToSchema;

/// Who the co-signing counterparty is. Its party id stays `CPTY`; only the story around it changes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CounterpartyRole {
    #[default]
    Insurer,
    CameraProvider,
    Sthiranet,
}

impl CounterpartyRole {
    pub const ALL: [Self; 3] = [Self::Insurer, Self::CameraProvider, Self::Sthiranet];

    pub fn name(self) -> &'static str {
        match self {
            Self::Insurer => "Insurer",
            Self::CameraProvider => "Camera provider",
            Self::Sthiranet => "Sthiranet",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Insurer => "Insures the warehouse operator. Witnesses every minute of footage as it is captured.",
            Self::CameraProvider => "Supplied and maintains the cameras. Witnesses every minute as it is captured.",
            Self::Sthiranet => "Network operator acting as a neutral witness of every minute as it is captured.",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RoleOption {
    pub role: CounterpartyRole,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PartyView {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CounterpartyView {
    pub id: String,
    pub name: String,
    pub description: String,
    pub role: CounterpartyRole,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct VendorView {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Whether the vendor was promoted to a member that co-signs new anchors.
    pub member: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CameraView {
    pub id: String,
    pub name: String,
    pub site: String,
    pub zone: String,
    pub key_id: String,
    /// The device's public key, as registered in the chain at genesis.
    pub device_key: String,
    /// Segments recorded but not yet sealed into a minute.
    pub pending: usize,
    /// Number of the last anchored batch (minute).
    pub last_seq: u64,
    pub last_head: String,
    /// Segments currently in the recorder's storage.
    pub stored: usize,
    pub members: Vec<Party>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClockView {
    /// Demo time now (RFC 3339 UTC). Simulated: it advances 10 s per recorded segment.
    pub now: String,
    /// Start of every camera chain.
    pub start: String,
    /// End of the last sealed minute.
    pub sealed_to: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct IncidentView {
    pub id: String,
    pub consignment: String,
    pub from: String,
    pub to: String,
    pub cams: Vec<String>,
}

/// Summary of the evidence package last exported for the vendor.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExportView {
    pub at: String,
    pub incident: String,
    pub cams: Vec<String>,
    pub batches: usize,
    pub segments: usize,
    pub anchors: usize,
    /// Size of the package (records, salts, proofs, anchors) as JSON.
    pub package_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LastVerify {
    pub verifier: Party,
    pub ok: bool,
    pub verdict: String,
    pub at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Info,
    Ok,
    Warn,
    Danger,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Activity {
    pub at: String,
    pub level: Level,
    pub actor: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StateView {
    /// Increments on every change; the UI refetches when it moves.
    pub version: u64,
    pub auto: bool,
    pub clock: ClockView,
    pub operator: PartyView,
    pub counterparty: CounterpartyView,
    pub vendor: VendorView,
    pub roles: Vec<RoleOption>,
    pub cameras: Vec<CameraView>,
    pub segment_seconds: u32,
    pub segments_per_batch: u32,
    pub incident: Option<IncidentView>,
    pub export: Option<ExportView>,
    pub last_verify: Option<LastVerify>,
    pub activity: Vec<Activity>,
    /// Number of envelopes the synchronizer has sequenced.
    pub envelopes: usize,
}

/// One segment as the recorder holds it right now.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SegmentView {
    pub id: String,
    pub seq: u64,
    /// The batch (minute) it was sealed into; `null` while pending.
    pub batch_seq: Option<u64>,
    pub index: usize,
    pub start: String,
    pub end: String,
    /// What the stored video shows; `null` if the video is gone.
    pub scene: Option<Scene>,
    /// The content hash in the signed record.
    pub content_hash: String,
    pub device_sig: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct FootageView {
    pub cam: String,
    pub segments: Vec<SegmentView>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchItemView {
    pub id: String,
    pub pos: String,
    pub label: String,
    pub text: String,
    pub salt: String,
    pub leaf_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchView {
    pub seq: u64,
    pub prev_head: String,
    pub head: String,
    pub merkle_root: String,
    pub event_count: u64,
    pub from_time: String,
    pub to_time: String,
    pub from_pos: String,
    pub to_pos: String,
    pub tree: MerkleTree,
    pub items: Vec<BatchItemView>,
}

impl From<&Batch> for BatchView {
    fn from(b: &Batch) -> Self {
        Self {
            seq: b.seq,
            prev_head: b.prev_head.clone(),
            head: b.head.clone(),
            merkle_root: b.merkle_root.clone(),
            event_count: b.event_count,
            from_time: b.from_time.clone(),
            to_time: b.to_time.clone(),
            from_pos: b.from_pos.clone(),
            to_pos: b.to_pos.clone(),
            tree: b.tree.clone(),
            items: b
                .items
                .iter()
                .map(|i| BatchItemView {
                    id: i.id.clone(),
                    pos: i.pos.clone(),
                    label: i.description.label.clone(),
                    text: i.description.text.clone(),
                    salt: i.salt.clone(),
                    leaf_hash: i.leaf_hash.clone(),
                })
                .collect(),
        }
    }
}

/// A sealed minute as the 3PL's node noted it, with the anchor from the 3PL's ledger copy.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchDetail {
    pub cam: String,
    pub seq: u64,
    pub total_batches: usize,
    pub batch: BatchView,
    pub anchor: Anchor,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LedgerView {
    pub party: Party,
    /// The chains stored on this party's node.
    pub chains: Vec<ChainCopy>,
    /// The synchronizer's most recent envelopes, newest first.
    pub synchronizer: Vec<Envelope>,
    pub total_envelopes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TamperResult {
    pub ok: bool,
    pub description: String,
}

/// The verdict on one 10-second slot the anchors say must exist.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SegmentVerdict {
    /// The segment number the anchor expects in this slot.
    pub seq: u64,
    pub batch_seq: u64,
    pub index: usize,
    pub start: String,
    pub end: String,
    pub status: SegmentStatus,
    /// Plain-language reason when the status is not `ok`.
    pub note: Option<String>,
    pub scene: Option<Scene>,
    /// Content hash in the record that was handed over.
    pub recorded_hash: Option<String>,
    /// SHA-256 of the video actually in storage.
    pub observed_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchVerdict {
    pub seq: u64,
    pub ok: bool,
    pub from_time: String,
    pub to_time: String,
    /// The records present hash to the anchored Merkle root.
    pub root_match: bool,
    /// The chain of heads up to this batch matches the anchors.
    pub head_match: bool,
    /// As many segments are present as the anchor says existed.
    pub count_match: bool,
    pub expected_count: u64,
    pub present_count: u64,
    pub anchored_root: String,
    pub recomputed_root: String,
    pub signatories: Vec<Party>,
    pub contract_id: String,
    /// Merkle tree over the records present, for display.
    pub tree: MerkleTree,
    /// Leaves of `tree` (by position) whose segment is not `ok`.
    pub bad_leaves: Vec<usize>,
    pub leaf_labels: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StatusCounts {
    pub ok: usize,
    pub modified: usize,
    pub missing: usize,
    pub replayed: usize,
    pub bad_signature: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CamVerdict {
    pub cam: String,
    pub ok: bool,
    pub verdict: String,
    /// Set when the anchors themselves could not be relied on.
    pub issue: Option<String>,
    pub counts: StatusCounts,
    pub segments: Vec<SegmentVerdict>,
    pub batches: Vec<BatchVerdict>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum VerifyMethod {
    /// A member checks the 3PL's storage against its own ledger copy.
    LedgerCopy,
    /// A non-member checks an evidence package (records, salts, proofs, anchors).
    EvidencePackage,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VerifyResponse {
    pub verifier: Party,
    pub verifier_name: String,
    pub method: VerifyMethod,
    /// Whose ledger copy supplied the anchors the footage was checked against.
    pub anchors_from: Party,
    pub incident: IncidentView,
    pub ok: bool,
    pub verdict: String,
    pub segments_checked: usize,
    pub at: String,
    pub cams: Vec<CamVerdict>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ErrorBody {
    pub error: String,
}

// ── Request bodies. Missing fields fall back to defaults; the actions validate them. ──

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(default)]
pub struct RecordRequest {
    /// Segments each camera records (1–60, default 6 = one minute).
    pub segments: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(default)]
pub struct RewriteRequest {
    pub actor: String,
    pub chain: String,
    pub seq: i64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TamperMode {
    /// Splice: replace the segment's video.
    #[default]
    Modify,
    /// Loop: copy earlier footage over the segment.
    Replay,
    /// Delete one segment.
    Delete,
    /// Delete the whole minute the segment belongs to.
    DeleteMinute,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(default)]
pub struct TamperRequest {
    pub cam: String,
    pub seq: u64,
    pub mode: TamperMode,
    /// Also re-hash and re-sign with the camera's key (for a deletion: renumber around the gap).
    pub cover: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(default)]
pub struct IncidentRequest {
    /// Window start, RFC 3339 UTC on a minute boundary. Default: 20 minutes after the start of the chains.
    pub from: Option<String>,
    /// Window end. Default: 30 minutes after the start of the chains.
    pub to: Option<String>,
    /// Cameras in scope. Default: all.
    pub cams: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(default)]
pub struct VerifyRequest {
    /// `CPTY` or `VENDOR`.
    pub verifier: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(default)]
pub struct CounterpartyRequest {
    pub role: CounterpartyRole,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(default)]
pub struct AutoRequest {
    pub on: bool,
}
