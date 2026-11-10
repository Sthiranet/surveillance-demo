//! The profile's own verification rules. They run on top of the generic ledger checks: those prove
//! a record is the one that was anchored; these prove the stored video still matches the record.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{Footage, SegmentRecord};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SegmentStatus {
    /// The stored bytes hash to the signed content hash.
    Ok,
    /// The stored bytes, or the record itself, are not what was captured.
    Modified,
    /// The segment is gone.
    Missing,
    /// The stored bytes are a copy of another segment's footage.
    Replayed,
    /// The record is not signed by the camera's registered device key.
    BadSignature,
}

/// The outcome of the profile checks for one segment record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentCheck {
    pub seq: u64,
    pub status: SegmentStatus,
    pub signature_ok: bool,
    /// The stored bytes hash to the record's `content_hash`.
    pub content_ok: bool,
    /// SHA-256 of the bytes found in storage, if any.
    pub observed_hash: Option<String>,
    /// The earlier segment whose footage these bytes duplicate.
    pub replay_of: Option<u64>,
}

/// Checks each record against the footage in storage and the device key registered for the camera.
///
/// Replay detection: genuine segments never share bytes, so a segment whose stored bytes equal an
/// earlier segment's is a loop of that footage, even if its record was re-signed to match.
pub fn check_segments<'a>(
    records: impl IntoIterator<Item = &'a SegmentRecord>,
    footage: &Footage,
    device_key: &str,
) -> Vec<SegmentCheck> {
    let mut first_with: HashMap<String, u64> = HashMap::new();
    for (seq, _) in footage.iter() {
        if let Some(h) = footage.hash(seq) {
            first_with.entry(h).or_insert(seq); // iteration is in seq order, so this is the earliest
        }
    }
    records
        .into_iter()
        .map(|r| {
            let signature_ok = r.signature_valid(device_key);
            let observed_hash = footage.hash(r.seq);
            let content_ok = observed_hash.as_deref() == Some(r.content_hash.as_str());
            let replay_of =
                observed_hash.as_ref().and_then(|h| first_with.get(h)).copied().filter(|first| *first < r.seq);
            let status = if observed_hash.is_none() {
                SegmentStatus::Missing
            } else if replay_of.is_some() {
                SegmentStatus::Replayed
            } else if !signature_ok {
                SegmentStatus::BadSignature
            } else if !content_ok {
                SegmentStatus::Modified
            } else {
                SegmentStatus::Ok
            };
            SegmentCheck { seq: r.seq, status, signature_ok, content_ok, observed_hash, replay_of }
        })
        .collect()
}

/// A break in a camera's sequence: after `after_seq` the next record is `next_seq`, or its start
/// time does not continue where the previous segment ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gap {
    pub after_seq: u64,
    pub next_seq: u64,
}

/// Contiguity: segment numbers must count up by one and each segment must start where the previous ended.
pub fn gaps<'a>(records: impl IntoIterator<Item = &'a SegmentRecord>) -> Vec<Gap> {
    let mut sorted: Vec<&SegmentRecord> = records.into_iter().collect();
    sorted.sort_by_key(|r| r.seq);
    sorted
        .windows(2)
        .filter(|w| w[1].seq != w[0].seq + 1 || w[1].start != w[0].end)
        .map(|w| Gap { after_seq: w[0].seq, next_seq: w[1].seq })
        .collect()
}
