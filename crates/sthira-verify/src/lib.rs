//! Node layer · sthira-verify. Two ways to check records against anchors, both generic over the
//! profile's [`Record`]:
//!
//! - [`verify_trail`]: a chain member takes a whole off-ledger trail, recomputes every leaf, Merkle
//!   root and head, and compares them with the anchors stored on ITS OWN ledger copy.
//! - [`verify_package`]: anyone, member or not, checks a window of records handed over as an
//!   [`EvidencePackage`] (records, salts, inclusion proofs and the anchors they sit under).
//!
//! Neither needs anything from the party that produced the records except the records themselves.

mod package;

pub use package::{
    EvidencePackage, ItemCheck, PackageBatchCheck, PackageReport, PackagedRecord, build_package, verify_anchor_chain,
    verify_package,
};

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sthira_core::{BatchMeta, Describe, Description, MerkleTree, Record, StoreRow, head_hash, leaf_hash, merkle_tree};
use sthira_ledger::Anchor;

/// A leaf that no longer hashes to the value recorded when its batch was sealed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LeafIssue {
    pub index: usize,
    pub record_id: String,
    pub pos: String,
    pub recorded: String,
    pub recomputed: String,
}

/// The values committed on the ledger.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AnchoredValues {
    pub root: String,
    pub head: String,
    pub prev_head: String,
    pub event_count: u64,
    pub from_time: String,
    pub to_time: String,
    pub signatories: Vec<String>,
    pub contract_id: String,
}

impl AnchoredValues {
    pub fn of(a: &Anchor) -> Self {
        Self {
            root: a.merkle_root.clone(),
            head: a.head.clone(),
            prev_head: a.prev_head.clone(),
            event_count: a.event_count,
            from_time: a.from_time.clone(),
            to_time: a.to_time.clone(),
            signatories: a.signatories.clone(),
            contract_id: a.contract_id.clone(),
        }
    }
}

/// The values recomputed from the trail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecomputedValues {
    pub root: String,
    pub head: String,
    pub prev_head: String,
    pub event_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LeafView {
    pub id: String,
    pub pos: String,
    pub index: usize,
    pub leaf: String,
    #[serde(flatten)]
    pub description: Description,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchCheck {
    pub seq: u64,
    pub ok: bool,
    pub root_match: bool,
    pub head_match: bool,
    pub count_match: bool,
    pub anchored: AnchoredValues,
    pub recomputed: RecomputedValues,
    pub leaf_issues: Vec<LeafIssue>,
    pub missing_indexes: Vec<usize>,
    pub tree: MerkleTree,
    pub leaves: Vec<LeafView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VerifyReport {
    pub ok: bool,
    pub batches: Vec<BatchCheck>,
    /// Batches whose root, head or count does not match.
    pub failed_seqs: Vec<u64>,
    /// Batch numbers present in the trail with no anchor on the ledger.
    pub unanchored: Vec<u64>,
    /// The first batch whose own contents changed (root or count mismatch).
    pub first_bad_seq: Option<u64>,
    pub event_count: usize,
}

/// Recomputes the trail `rows` of chain `chain_id` and compares it with `anchors`, which must come
/// from the verifier's own ledger copy.
pub fn verify_trail<R: Record + Describe>(
    chain_id: &str,
    genesis_head: &str,
    anchors: &[Anchor],
    rows: &[StoreRow<R>],
) -> VerifyReport {
    let mut by_seq: BTreeMap<u64, Vec<&StoreRow<R>>> = BTreeMap::new();
    for r in rows {
        by_seq.entry(r.batch_seq).or_default().push(r);
    }

    let mut prev_head = genesis_head.to_string();
    let mut batches = Vec::with_capacity(anchors.len());
    for a in anchors {
        let mut evs: Vec<&StoreRow<R>> = by_seq.get(&a.seq).cloned().unwrap_or_default();
        evs.sort_by_key(|r| r.index);
        let leaves: Vec<String> = evs.iter().map(|r| leaf_hash(&r.salt, &r.record)).collect();
        let tree = merkle_tree(&leaves);
        let meta = BatchMeta {
            chain: chain_id.to_string(),
            seq: a.seq,
            from_pos: a.from_pos.clone(),
            to_pos: a.to_pos.clone(),
            from_time: a.from_time.clone(),
            to_time: a.to_time.clone(),
            event_count: evs.len() as u64,
        };
        let head = head_hash(&prev_head, &tree.hash, &meta);

        let leaf_issues = evs
            .iter()
            .zip(&leaves)
            .enumerate()
            .filter(|(_, (r, l))| r.leaf_hash != **l)
            .map(|(i, (r, l))| LeafIssue {
                index: i,
                record_id: r.id.clone(),
                pos: r.record.pos(),
                recorded: r.leaf_hash.clone(),
                recomputed: l.clone(),
            })
            .collect();
        let present: BTreeSet<usize> = evs.iter().map(|r| r.index).collect();
        let missing_indexes = (0..a.event_count as usize).filter(|i| !present.contains(i)).collect();

        let root_match = tree.hash == a.merkle_root;
        let head_match = head == a.head;
        let count_match = evs.len() as u64 == a.event_count;
        batches.push(BatchCheck {
            seq: a.seq,
            ok: root_match && head_match && count_match,
            root_match,
            head_match,
            count_match,
            anchored: AnchoredValues::of(a),
            recomputed: RecomputedValues {
                root: tree.hash.clone(),
                head: head.clone(),
                prev_head: prev_head.clone(),
                event_count: evs.len() as u64,
            },
            leaf_issues,
            missing_indexes,
            leaves: evs
                .iter()
                .zip(&leaves)
                .map(|(r, l)| LeafView {
                    id: r.id.clone(),
                    pos: r.record.pos(),
                    index: r.index,
                    leaf: l.clone(),
                    description: r.record.describe(),
                })
                .collect(),
            tree,
        });
        prev_head = head; // heads chain: one altered batch breaks every head after it
    }

    let anchored: BTreeSet<u64> = anchors.iter().map(|a| a.seq).collect();
    let unanchored: Vec<u64> = by_seq.keys().copied().filter(|s| !anchored.contains(s)).collect();
    let failed_seqs: Vec<u64> = batches.iter().filter(|b| !b.ok).map(|b| b.seq).collect();
    let first_bad_seq = batches.iter().find(|b| !b.root_match || !b.count_match).map(|b| b.seq);
    VerifyReport {
        ok: failed_seqs.is_empty() && unanchored.is_empty(),
        batches,
        failed_seqs,
        unanchored,
        first_bad_seq,
        event_count: rows.len(),
    }
}

#[cfg(test)]
pub(crate) mod testkit {
    //! A stand-in profile record and a sealed chain built by hand, shared by this crate's tests.
    use serde::{Deserialize, Serialize};
    use sthira_core::{
        BatchMeta, Describe, Description, Record, StoreRow, genesis_head, head_hash, leaf_hash, merkle_root,
    };
    use sthira_ledger::Anchor;

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct Note {
        pub chain: String,
        pub n: u64,
        pub text: String,
    }

    impl Record for Note {
        fn chain(&self) -> &str {
            &self.chain
        }
        fn pos(&self) -> String {
            format!("{:04}", self.n)
        }
    }

    impl Describe for Note {
        fn describe(&self) -> Description {
            Description { kind: "note".into(), label: "Note".into(), text: self.text.clone() }
        }
    }

    pub const CHAIN: &str = "C1";

    pub struct Sealed {
        pub genesis: String,
        pub anchors: Vec<Anchor>,
        pub rows: Vec<StoreRow<Note>>,
    }

    impl Sealed {
        pub fn leaves(&self, seq: u64) -> Option<Vec<String>> {
            let v: Vec<String> = self.rows.iter().filter(|r| r.batch_seq == seq).map(|r| r.leaf_hash.clone()).collect();
            self.anchors.iter().any(|a| a.seq == seq).then_some(v)
        }
    }

    /// A chain with one batch per entry of `sizes`, anchored as an honest node would.
    pub fn sealed(sizes: &[usize]) -> Sealed {
        let genesis = genesis_head(CHAIN);
        let (mut anchors, mut rows) = (Vec::new(), Vec::new());
        let (mut prev, mut n) = (genesis.clone(), 0u64);
        for (b, size) in sizes.iter().enumerate() {
            let seq = b as u64 + 1;
            let first = n + 1;
            let mut leaves = Vec::new();
            for index in 0..*size {
                n += 1;
                let record = Note { chain: CHAIN.into(), n, text: format!("note {n}") };
                let salt = format!("{:02x}", n % 251).repeat(32);
                let leaf = leaf_hash(&salt, &record);
                leaves.push(leaf.clone());
                rows.push(StoreRow {
                    id: format!("{CHAIN}-{n:05}"),
                    chain: CHAIN.into(),
                    batch_seq: seq,
                    index,
                    record,
                    salt,
                    leaf_hash: leaf,
                });
            }
            let root = merkle_root(&leaves);
            let meta = BatchMeta {
                chain: CHAIN.into(),
                seq,
                from_pos: format!("{first:04}"),
                to_pos: format!("{n:04}"),
                from_time: format!("2026-11-10T08:{b:02}:00.000000Z"),
                to_time: format!("2026-11-10T08:{:02}:00.000000Z", b + 1),
                event_count: *size as u64,
            };
            let head = head_hash(&prev, &root, &meta);
            anchors.push(Anchor {
                contract_id: format!("#{seq}"),
                template: "Test:Anchor".into(),
                chain_id: CHAIN.into(),
                seq,
                prev_head: prev.clone(),
                head: head.clone(),
                merkle_root: root,
                event_count: meta.event_count,
                from_pos: meta.from_pos,
                to_pos: meta.to_pos,
                from_time: meta.from_time,
                to_time: meta.to_time,
                signatories: vec!["A".into(), "B".into()],
                update_id: format!("upd-{seq}"),
                recorded_at: String::new(),
            });
            prev = head;
        }
        Sealed { genesis, anchors, rows }
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;

    #[test]
    fn a_clean_trail_verifies() {
        let s = sealed(&[6, 6, 0, 3]);
        let r = verify_trail(CHAIN, &s.genesis, &s.anchors, &s.rows);
        assert!(r.ok, "{:?}", r.failed_seqs);
        assert_eq!((r.batches.len(), r.event_count), (4, 15));
    }

    #[test]
    fn an_edit_is_pinpointed_and_later_heads_cascade() {
        let mut s = sealed(&[6, 6, 6]);
        s.rows[7].record.text = "edited".into();
        let r = verify_trail(CHAIN, &s.genesis, &s.anchors, &s.rows);
        assert_eq!(r.first_bad_seq, Some(2));
        assert!(r.batches[0].ok);
        assert_eq!(r.batches[1].leaf_issues[0].record_id, s.rows[7].id);
        assert!(r.batches[2].root_match && !r.batches[2].head_match);
    }

    #[test]
    fn a_covered_edit_still_breaks_the_root() {
        let mut s = sealed(&[6, 6]);
        s.rows[2].record.text = "edited".into();
        s.rows[2].leaf_hash = leaf_hash(&s.rows[2].salt, &s.rows[2].record);
        let r = verify_trail(CHAIN, &s.genesis, &s.anchors, &s.rows);
        assert!(r.batches[0].leaf_issues.is_empty());
        assert!(!r.batches[0].root_match);
    }

    #[test]
    fn a_deletion_is_a_count_mismatch_with_the_missing_index() {
        let mut s = sealed(&[6, 6]);
        s.rows.remove(9);
        let r = verify_trail(CHAIN, &s.genesis, &s.anchors, &s.rows);
        let b = &r.batches[1];
        assert!(!b.count_match && !b.root_match);
        assert_eq!(b.missing_indexes, [3]);
    }
}
