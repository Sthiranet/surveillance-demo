//! Evidence packages: a window of records a non-member can verify without a node.
//!
//! The package carries everything needed and nothing private beyond the window:
//! 1. the chain's anchors up to the end of the window, so the heads can be chained from genesis;
//! 2. per record: the record, its salt, its leaf index and its RFC 6962 inclusion proof.
//!
//! The verifier checks the anchors against a ledger copy it trusts (its own, or a co-signer's), so
//! it never has to take the exporter's word for them. Completeness comes from the anchors too: a
//! batch that was anchored with `event_count` leaves must arrive with that many records.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sthira_core::{
    BatchMeta, MerkleTree, Record, StoreRow, head_hash, inclusion_proof, leaf_hash, merkle_tree, verify_inclusion,
};
use sthira_ledger::Anchor;

use crate::AnchoredValues;

/// One record of a package, with what is needed to place it under its batch's Merkle root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackagedRecord<R> {
    pub id: String,
    pub batch_seq: u64,
    /// Position of the leaf in its batch.
    pub index: usize,
    pub record: R,
    pub salt: String,
    /// Sibling hashes from the leaf up to the root.
    pub proof: Vec<String>,
}

/// The records of batches `from_seq..=to_seq` of one chain, ready to hand to a verifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidencePackage<R> {
    pub chain_id: String,
    pub genesis_head: String,
    pub from_seq: u64,
    pub to_seq: u64,
    /// Anchors `1..=to_seq`: hashes only, so the heads chain from genesis.
    pub anchors: Vec<Anchor>,
    pub items: Vec<PackagedRecord<R>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ItemCheck {
    pub id: String,
    pub index: usize,
    pub pos: String,
    /// The leaf recomputed from the record and its salt.
    pub leaf: String,
    /// Whether that leaf sits at `index` under the anchored Merkle root.
    pub included: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackageBatchCheck {
    pub seq: u64,
    pub ok: bool,
    /// The package holds as many records as the anchor says the batch had.
    pub count_match: bool,
    /// The records present hash to the anchored root (only possible when none is missing).
    pub root_match: bool,
    pub anchored: AnchoredValues,
    pub present: u64,
    pub recomputed_root: String,
    pub missing_indexes: Vec<usize>,
    pub items: Vec<ItemCheck>,
    /// The tree over the records present, for display.
    pub tree: MerkleTree,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackageReport {
    pub ok: bool,
    /// The package's anchors chain from genesis, each head recomputed.
    pub chain_ok: bool,
    /// The package's anchors are the ones on the trusted ledger copy.
    pub anchors_trusted: bool,
    /// Why the anchors were rejected, if they were.
    pub issue: Option<String>,
    pub batches: Vec<PackageBatchCheck>,
    pub failed_seqs: Vec<u64>,
    pub item_count: usize,
}

/// Builds the package for batches `from_seq..=to_seq` from the exporter's store.
///
/// `sealed_leaves(seq)` returns the batch's leaf hashes as noted when it was sealed (the node's
/// batch table); the proofs are computed from them. A record the store no longer holds is simply
/// absent from the package, which the verifier then reports as missing.
pub fn build_package<R: Record>(
    chain_id: &str,
    genesis_head: &str,
    anchors: &[Anchor],
    rows: &[StoreRow<R>],
    sealed_leaves: &dyn Fn(u64) -> Option<Vec<String>>,
    from_seq: u64,
    to_seq: u64,
) -> EvidencePackage<R> {
    let mut items = Vec::new();
    for seq in from_seq..=to_seq {
        let leaves = sealed_leaves(seq).unwrap_or_default();
        let mut batch: Vec<&StoreRow<R>> = rows.iter().filter(|r| r.batch_seq == seq).collect();
        batch.sort_by_key(|r| r.index);
        for r in batch {
            items.push(PackagedRecord {
                id: r.id.clone(),
                batch_seq: seq,
                index: r.index,
                record: r.record.clone(),
                salt: r.salt.clone(),
                proof: inclusion_proof(&leaves, r.index).unwrap_or_default(),
            });
        }
    }
    EvidencePackage {
        chain_id: chain_id.to_string(),
        genesis_head: genesis_head.to_string(),
        from_seq,
        to_seq,
        anchors: anchors.iter().filter(|a| a.seq <= to_seq).cloned().collect(),
        items,
    }
}

/// Checks that `anchors` are batches `1, 2, 3…` of `chain_id`, each head recomputed from the
/// previous head, the Merkle root and the batch metadata, starting at `genesis_head`.
pub fn verify_anchor_chain(chain_id: &str, genesis_head: &str, anchors: &[Anchor]) -> Result<(), String> {
    let mut prev = genesis_head.to_string();
    for (i, a) in anchors.iter().enumerate() {
        let expected = i as u64 + 1;
        if a.chain_id != chain_id {
            return Err(format!("anchor #{} belongs to chain {}, not {chain_id}", a.seq, a.chain_id));
        }
        if a.seq != expected {
            return Err(format!("expected anchor #{expected}, found #{}", a.seq));
        }
        if a.prev_head != prev {
            return Err(format!("anchor #{} does not continue the head of #{}", a.seq, a.seq - 1));
        }
        let meta = BatchMeta {
            chain: chain_id.to_string(),
            seq: a.seq,
            from_pos: a.from_pos.clone(),
            to_pos: a.to_pos.clone(),
            from_time: a.from_time.clone(),
            to_time: a.to_time.clone(),
            event_count: a.event_count,
        };
        if head_hash(&prev, &a.merkle_root, &meta) != a.head {
            return Err(format!("anchor #{}: the head does not match its root and metadata", a.seq));
        }
        prev = a.head.clone();
    }
    Ok(())
}

/// The fields of an anchor that commit to content. Signatories may grow when a member joins.
fn same_commitment(a: &Anchor, b: &Anchor) -> bool {
    (a.seq, &a.prev_head, &a.head, &a.merkle_root, a.event_count)
        == (b.seq, &b.prev_head, &b.head, &b.merkle_root, b.event_count)
        && (&a.from_pos, &a.to_pos, &a.from_time, &a.to_time) == (&b.from_pos, &b.to_pos, &b.from_time, &b.to_time)
}

fn check_anchors<R>(pkg: &EvidencePackage<R>, trusted: &[Anchor]) -> (bool, bool, Option<String>) {
    let chain = verify_anchor_chain(&pkg.chain_id, &pkg.genesis_head, &pkg.anchors);
    let window = if pkg.from_seq == 0 || pkg.from_seq > pkg.to_seq {
        Err(format!("the package window #{}–#{} is not valid", pkg.from_seq, pkg.to_seq))
    } else if (pkg.anchors.len() as u64) < pkg.to_seq {
        Err(format!("the package stops at anchor #{} but claims a window up to #{}", pkg.anchors.len(), pkg.to_seq))
    } else {
        Ok(())
    };
    let trust = pkg.anchors.iter().try_for_each(|a| match trusted.iter().find(|t| t.seq == a.seq) {
        Some(t) if same_commitment(a, t) => Ok(()),
        Some(_) => Err(format!("anchor #{} differs from the one on the verifier's ledger copy", a.seq)),
        None => Err(format!("anchor #{} is not on the verifier's ledger copy", a.seq)),
    });
    let chain_ok = chain.is_ok() && window.is_ok();
    let issue = chain.err().or(window.err()).or(trust.clone().err());
    (chain_ok, trust.is_ok(), issue)
}

/// Verifies `pkg` against `trusted`, the anchors on a ledger copy the verifier relies on.
///
/// Per record: the leaf recomputed from record and salt must sit at its index under the anchored
/// root. Per batch: the package must hold exactly `event_count` records. Any altered record,
/// proof or anchor, and any missing record, makes the report fail.
pub fn verify_package<R: Record>(pkg: &EvidencePackage<R>, trusted: &[Anchor]) -> PackageReport {
    let (chain_ok, anchors_trusted, issue) = check_anchors(pkg, trusted);

    let mut by_seq: BTreeMap<u64, Vec<&PackagedRecord<R>>> = BTreeMap::new();
    for it in &pkg.items {
        by_seq.entry(it.batch_seq).or_default().push(it);
    }
    let stray = by_seq.keys().any(|s| *s < pkg.from_seq || *s > pkg.to_seq);

    let mut batches = Vec::new();
    for a in pkg.anchors.iter().filter(|a| a.seq >= pkg.from_seq && a.seq <= pkg.to_seq) {
        let mut recs = by_seq.get(&a.seq).cloned().unwrap_or_default();
        recs.sort_by_key(|r| r.index);
        let size = a.event_count as usize;
        let mut seen = BTreeSet::new();
        let items: Vec<ItemCheck> = recs
            .iter()
            .map(|r| {
                let leaf = leaf_hash(&r.salt, &r.record);
                let fresh = seen.insert(r.index); // a second record claiming the same leaf is rejected
                let included = fresh && verify_inclusion(&leaf, r.index, size, &r.proof, &a.merkle_root);
                ItemCheck { id: r.id.clone(), index: r.index, pos: r.record.pos(), leaf, included }
            })
            .collect();
        let missing_indexes: Vec<usize> = (0..size).filter(|i| !seen.contains(i)).collect();
        let leaves: Vec<String> = items.iter().map(|i| i.leaf.clone()).collect();
        let tree = merkle_tree(&leaves);
        let count_match = items.len() == size && missing_indexes.is_empty();
        let root_match = count_match && tree.hash == a.merkle_root;
        batches.push(PackageBatchCheck {
            seq: a.seq,
            ok: count_match && root_match && items.iter().all(|i| i.included),
            count_match,
            root_match,
            anchored: AnchoredValues::of(a),
            present: items.len() as u64,
            recomputed_root: tree.hash.clone(),
            missing_indexes,
            items,
            tree,
        });
    }

    let failed_seqs: Vec<u64> = batches.iter().filter(|b| !b.ok).map(|b| b.seq).collect();
    let covered = batches.len() as u64 == (pkg.to_seq + 1).saturating_sub(pkg.from_seq);
    PackageReport {
        ok: chain_ok && anchors_trusted && covered && !stray && failed_seqs.is_empty(),
        chain_ok,
        anchors_trusted,
        issue,
        batches,
        failed_seqs,
        item_count: pkg.items.len(),
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::testkit::*;

    fn package(s: &Sealed, from: u64, to: u64) -> EvidencePackage<Note> {
        build_package(CHAIN, &s.genesis, &s.anchors, &s.rows, &|seq| s.leaves(seq), from, to)
    }

    #[test]
    fn a_valid_window_verifies_without_the_rest_of_the_trail() {
        let s = sealed(&[6, 6, 6, 6]);
        let pkg = package(&s, 2, 3);
        assert_eq!((pkg.anchors.len(), pkg.items.len()), (3, 12));
        let r = verify_package(&pkg, &s.anchors);
        assert!(r.ok && r.chain_ok && r.anchors_trusted, "{:?}", r.issue);
        assert_eq!(r.batches.iter().map(|b| b.seq).collect::<Vec<_>>(), [2, 3]);
        assert!(r.batches.iter().all(|b| b.root_match && b.items.iter().all(|i| i.included)));
    }

    #[test]
    fn an_empty_batch_in_the_window_is_fine() {
        let s = sealed(&[3, 0, 2]);
        assert!(verify_package(&package(&s, 1, 3), &s.anchors).ok);
    }

    #[test]
    fn a_missing_record_is_reported_and_the_rest_still_prove() {
        let mut s = sealed(&[6, 6]);
        let sealed_leaves = s.leaves(2).unwrap();
        s.rows.remove(8); // batch 2, index 2
        let pkg =
            build_package(CHAIN, &s.genesis, &s.anchors, &s.rows, &|q| (q == 2).then(|| sealed_leaves.clone()), 2, 2);
        let r = verify_package(&pkg, &s.anchors);
        let b = &r.batches[0];
        assert!(!r.ok && !b.count_match);
        assert_eq!(b.missing_indexes, [2]);
        assert!(b.items.iter().all(|i| i.included), "the five that remain are still proven");
    }

    #[test]
    fn anchors_must_chain_and_match_the_trusted_copy() {
        let s = sealed(&[6, 6, 6]);
        assert!(verify_anchor_chain(CHAIN, &s.genesis, &s.anchors).is_ok());
        assert!(verify_anchor_chain(CHAIN, &s.genesis, &s.anchors[1..]).is_err());
        assert!(verify_anchor_chain("OTHER", &s.genesis, &s.anchors).is_err());

        // The exporter swaps in a root of its own: the chain breaks and the trusted copy disagrees.
        let mut pkg = package(&s, 1, 3);
        pkg.anchors[1].merkle_root = "00".repeat(32);
        let r = verify_package(&pkg, &s.anchors);
        assert!(!r.ok && !r.chain_ok && !r.anchors_trusted);

        // A self-consistent chain the verifier's ledger copy has never seen is rejected too.
        let other = sealed(&[5, 5, 5]);
        let r = verify_package(&package(&other, 1, 3), &s.anchors);
        assert!(r.chain_ok && !r.anchors_trusted && !r.ok);

        // Leaving the last batch out of the window's anchors is caught.
        let mut pkg = package(&s, 1, 3);
        pkg.anchors.pop();
        assert!(!verify_package(&pkg, &s.anchors).ok);
    }

    #[test]
    fn a_duplicated_record_cannot_fill_a_gap() {
        let s = sealed(&[6]);
        let mut pkg = package(&s, 1, 1);
        pkg.items[3] = pkg.items[2].clone();
        let r = verify_package(&pkg, &s.anchors);
        assert!(!r.ok);
        assert_eq!(r.batches[0].missing_indexes, [3]);
    }

    fn flip_hex(h: &mut String, at: usize) {
        let i = at % h.len();
        let c = if &h[i..=i] == "0" { "1" } else { "0" };
        h.replace_range(i..=i, c);
    }

    proptest! {
        #[test]
        fn valid_packages_verify(sizes in prop::collection::vec(0usize..9, 1..6), a in 0usize..6, b in 0usize..6) {
            let s = sealed(&sizes);
            let (lo, hi) = (a.min(b) % sizes.len(), a.max(b) % sizes.len());
            let (from, to) = (lo.min(hi) as u64 + 1, lo.max(hi) as u64 + 1);
            let r = verify_package(&package(&s, from, to), &s.anchors);
            prop_assert!(r.ok, "{:?}", r.issue);
        }

        #[test]
        fn any_alteration_fails(sizes in prop::collection::vec(1usize..9, 1..5), pick in any::<usize>(), what in 0u8..6, at in any::<usize>()) {
            let s = sealed(&sizes);
            let mut pkg = package(&s, 1, sizes.len() as u64);
            let i = pick % pkg.items.len();
            let size = sizes[pkg.items[i].batch_seq as usize - 1];
            match what {
                0 => pkg.items[i].record.text.push('!'),
                1 => pkg.items[i].record.n += 1,
                2 => flip_hex(&mut pkg.items[i].salt, at),
                3 if size > 1 => { let p = at % pkg.items[i].proof.len(); flip_hex(&mut pkg.items[i].proof[p], at) }
                4 => { let k = pick % pkg.anchors.len(); flip_hex(&mut pkg.anchors[k].merkle_root, at) }
                _ => { pkg.items.remove(i); }
            }
            prop_assert!(!verify_package(&pkg, &s.anchors).ok);
        }
    }
}
