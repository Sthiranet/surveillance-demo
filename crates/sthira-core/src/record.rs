//! The seam between the kernel/node layers and an industry profile (connector layer).
//!
//! A profile defines what gets anchored (a ledger change, a video segment, a threat indicator) and
//! implements [`Record`] and [`Describe`] for it. `sthira-node` and `sthira-verify` are written
//! against these traits only, so they never name a profile's types.

use chrono::{DateTime, SecondsFormat, Utc};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Anything a profile anchors. Its canonical JSON (RFC 8785) is what a leaf commits to.
pub trait Record: Serialize + DeserializeOwned + Clone + Send + Sync + 'static {
    /// The chain this record belongs to.
    fn chain(&self) -> &str;
    /// Position in the source, ordered within a chain: a log position (LSN/SCN), a segment number…
    fn pos(&self) -> String;
}

/// The business meaning of a record, for screens and reports.
pub trait Describe {
    fn describe(&self) -> Description;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Description {
    pub kind: String,
    pub label: String,
    pub text: String,
}

/// One row of a node's off-ledger store: the record, its salt and the leaf hash recorded when the
/// batch was sealed. None of this goes on the ledger.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreRow<R> {
    pub id: String,
    pub chain: String,
    pub batch_seq: u64,
    /// Position of the leaf in its batch.
    pub index: usize,
    pub record: R,
    pub salt: String,
    pub leaf_hash: String,
}

/// RFC 3339 UTC with microseconds (spec v1) from Unix milliseconds.
pub fn iso_micros(ms: i64) -> String {
    DateTime::<Utc>::from_timestamp_millis(ms).unwrap_or_default().to_rfc3339_opts(SecondsFormat::Micros, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamps_follow_spec_v1() {
        assert_eq!(iso_micros(0), "1970-01-01T00:00:00.000000Z");
        assert_eq!(iso_micros(1_762_675_210_250), "2025-11-09T08:00:10.250000Z");
    }
}
