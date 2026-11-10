//! Node layer · one chain on a participant node. Records wait in `pending`; [`ChainNode::seal`]
//! closes a batch, hashes it into a Merkle tree, chains the head, and appends an anchor on the
//! ledger. The records themselves stay in this node's off-ledger store (`store`).
//!
//! The node knows nothing about what a record is: it works through the [`Record`] and
//! [`Describe`] traits a profile implements, and through the [`AnchorLedger`] trait.
//!
//! An empty batch is sealed and anchored too, so a quiet period is visible on the ledger.

use serde::{Deserialize, Serialize};
use sthira_core::{
    BatchMeta, Describe, Description, MerkleTree, Record, StoreRow, genesis_head, head_hash, leaf_hash, merkle_tree,
    new_salt,
};
use sthira_ledger::{Anchor, AnchorInput, AnchorLedger, LedgerError};

/// A record waiting for the next batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingRecord<R> {
    pub id: String,
    pub record: R,
}

/// What was hashed for one leaf, as the node noted it when the batch was sealed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchItem {
    pub id: String,
    pub pos: String,
    #[serde(flatten)]
    pub description: Description,
    pub salt: String,
    pub leaf_hash: String,
}

/// A row of the node's own batch table.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Batch {
    pub seq: u64,
    pub prev_head: String,
    pub head: String,
    pub merkle_root: String,
    pub event_count: u64,
    pub from_time: String,
    pub to_time: String,
    pub from_pos: String,
    pub to_pos: String,
    pub meta: BatchMeta,
    pub tree: MerkleTree,
    pub sealed_at: String,
    pub contract_id: String,
    pub update_id: String,
    pub items: Vec<BatchItem>,
}

impl Batch {
    /// The leaf hashes in batch order, as sealed.
    pub fn leaves(&self) -> Vec<String> {
        self.items.iter().map(|i| i.leaf_hash.clone()).collect()
    }
}

pub struct ChainNode<R> {
    /// The party operating this node; it submits the chain's anchors.
    pub party: String,
    pub chain_id: String,
    pub genesis: String,
    /// Start of the first batch window (RFC 3339 UTC, microseconds).
    pub chain_start: String,
    pub pending: Vec<PendingRecord<R>>,
    /// Off-ledger store: one row per sealed record.
    pub store: Vec<StoreRow<R>>,
    /// The node's own batch table.
    pub batches: Vec<Batch>,
    record_seq: u64,
}

impl<R: Record + Describe> ChainNode<R> {
    pub fn new(party: impl Into<String>, chain_id: impl Into<String>, chain_start: impl Into<String>) -> Self {
        let chain_id = chain_id.into();
        Self {
            party: party.into(),
            genesis: genesis_head(&chain_id),
            chain_id,
            chain_start: chain_start.into(),
            pending: Vec::new(),
            store: Vec::new(),
            batches: Vec::new(),
            record_seq: 0,
        }
    }

    pub fn last_batch(&self) -> Option<&Batch> {
        self.batches.last()
    }

    pub fn last_seq(&self) -> u64 {
        self.last_batch().map_or(0, |b| b.seq)
    }

    pub fn last_head(&self) -> &str {
        self.last_batch().map_or(&self.genesis, |b| &b.head)
    }

    pub fn batch(&self, seq: u64) -> Option<&Batch> {
        self.batches.iter().find(|b| b.seq == seq)
    }

    /// Takes a record from the connector layer and queues it for the next batch.
    pub fn ingest(&mut self, record: R) -> &PendingRecord<R> {
        self.record_seq += 1;
        self.pending.push(PendingRecord { id: format!("{}-{:05}", self.chain_id, self.record_seq), record });
        self.pending.last().expect("just pushed")
    }

    /// Seals every pending record into the next batch, whose window ends at `to_time`, and anchors
    /// it. Fails without changing anything if any member's node refuses to co-sign.
    pub fn seal(&mut self, ledger: &mut dyn AnchorLedger, to_time: &str) -> Result<(&Batch, Anchor), LedgerError> {
        let last = self.last_batch();
        let seq = last.map_or(0, |b| b.seq) + 1;
        let prev_head = last.map_or_else(|| self.genesis.clone(), |b| b.head.clone());
        let from_time = last.map_or_else(|| self.chain_start.clone(), |b| b.to_time.clone());
        let prev_pos = last.map(|b| b.to_pos.clone()).unwrap_or_default();
        let to_time = to_time.to_string();

        let records = &self.pending;
        let salts: Vec<String> = records.iter().map(|_| new_salt()).collect();
        let leaves: Vec<String> = records.iter().zip(&salts).map(|(r, s)| leaf_hash(s, &r.record)).collect();
        let tree = merkle_tree(&leaves);
        let from_pos = records.first().map_or_else(|| prev_pos.clone(), |r| r.record.pos());
        let to_pos = records.last().map_or(prev_pos, |r| r.record.pos());
        let meta = BatchMeta {
            chain: self.chain_id.clone(),
            seq,
            from_pos: from_pos.clone(),
            to_pos: to_pos.clone(),
            from_time: from_time.clone(),
            to_time: to_time.clone(),
            event_count: records.len() as u64,
        };
        let head = head_hash(&prev_head, &tree.hash, &meta);

        let anchor = ledger.append(
            &self.chain_id,
            &self.party,
            AnchorInput {
                seq,
                prev_head: prev_head.clone(),
                head: head.clone(),
                merkle_root: tree.hash.clone(),
                event_count: records.len() as u64,
                from_pos: from_pos.clone(),
                to_pos: to_pos.clone(),
                from_time: from_time.clone(),
                to_time: to_time.clone(),
            },
        )?;

        let pending = std::mem::take(&mut self.pending);
        let mut items = Vec::with_capacity(pending.len());
        for (i, ((rec, salt), leaf)) in pending.into_iter().zip(salts).zip(leaves).enumerate() {
            items.push(BatchItem {
                id: rec.id.clone(),
                pos: rec.record.pos(),
                description: rec.record.describe(),
                salt: salt.clone(),
                leaf_hash: leaf.clone(),
            });
            self.store.push(StoreRow {
                id: rec.id,
                chain: self.chain_id.clone(),
                batch_seq: seq,
                index: i,
                record: rec.record,
                salt,
                leaf_hash: leaf,
            });
        }
        self.batches.push(Batch {
            seq,
            prev_head,
            head,
            merkle_root: tree.hash.clone(),
            event_count: meta.event_count,
            from_time,
            to_time: to_time.clone(),
            from_pos,
            to_pos,
            meta,
            tree,
            sealed_at: to_time,
            contract_id: anchor.contract_id.clone(),
            update_id: anchor.update_id.clone(),
            items,
        });
        Ok((self.batches.last().expect("just pushed"), anchor))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use sthira_ledger::{ChainSpec, SimLedger};

    /// A stand-in profile record: the node needs nothing more than the two traits.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct Note {
        chain: String,
        n: u64,
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
            Description { kind: "note".into(), label: "Note".into(), text: format!("#{}", self.n) }
        }
    }

    fn setup() -> (SimLedger, ChainNode<Note>) {
        let mut ledger = SimLedger::new();
        ledger.add_party("A");
        ledger.add_party("B");
        let node = ChainNode::new("A", "C1", "2026-11-10T08:00:00.000000Z");
        ledger
            .create_chain(&ChainSpec {
                chain_id: "C1".into(),
                template: "Test:Anchor".into(),
                profile: "test/v1".into(),
                submitter: "A".into(),
                members: vec!["A".into(), "B".into()],
                genesis_head: node.genesis.clone(),
                attributes: BTreeMap::new(),
            })
            .unwrap();
        (ledger, node)
    }

    #[test]
    fn seal_anchors_and_chains() {
        let (mut ledger, mut node) = setup();
        for n in 1..=10 {
            node.ingest(Note { chain: "C1".into(), n });
        }
        assert_eq!(node.pending[0].id, "C1-00001");
        let genesis = node.genesis.clone();
        let (b1, a1) = node.seal(&mut ledger, "2026-11-10T08:01:00.000000Z").unwrap();
        assert_eq!((b1.seq, b1.event_count, b1.items.len()), (1, 10, 10));
        assert_eq!((b1.from_pos.as_str(), b1.to_pos.as_str()), ("0001", "0010"));
        assert_eq!(a1.merkle_root, b1.merkle_root);
        assert_eq!(a1.prev_head, genesis);
        assert_eq!(a1.signatories, ["A", "B"]);
        assert_eq!(b1.items[0].description.label, "Note");
        let h1 = b1.head.clone();

        let (b2, a2) = node.seal(&mut ledger, "2026-11-10T08:02:00.000000Z").unwrap(); // empty batch
        assert_eq!(b2.event_count, 0);
        assert_eq!(a2.prev_head, h1);
        assert_eq!(b2.from_pos, b2.to_pos);
        assert_eq!(b2.from_time, "2026-11-10T08:01:00.000000Z");
        assert_eq!(node.store.len(), 10);
        assert!(node.pending.is_empty());
        assert_eq!(ledger.copy("B", "C1").unwrap().last_seq, 2);
    }

    #[test]
    fn a_refused_anchor_changes_nothing() {
        let (mut ledger, mut node) = setup();
        node.ingest(Note { chain: "C1".into(), n: 1 });
        node.party = "B".into(); // not the submitter
        assert!(node.seal(&mut ledger, "2026-11-10T08:01:00.000000Z").is_err());
        assert_eq!((node.pending.len(), node.store.len(), node.batches.len()), (1, 0, 0));
    }

    #[test]
    fn store_rows_serialise_for_export() {
        let (mut ledger, mut node) = setup();
        node.ingest(Note { chain: "C1".into(), n: 1 });
        node.seal(&mut ledger, "2026-11-10T08:01:00.000000Z").unwrap();
        let v = serde_json::to_value(&node.store[0]).unwrap();
        assert_eq!(v["batchSeq"], 1);
        assert_eq!(v["record"]["n"], 1);
    }
}
