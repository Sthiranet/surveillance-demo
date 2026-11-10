//! Kernel · hashing spec v1 (see the build plan).
//!
//! ```text
//! record  = JCS(whatever a profile anchors: it implements [`Record`])
//! salt    = 32 random bytes per record, kept in the off-ledger store, never on the ledger
//! leaf    = H(0x00 || salt || record)
//! node    = H(0x01 || left || right)
//! root    = RFC 6962 Merkle tree hash over the batch's leaves in record order
//!           (an empty batch has root H(""))
//! meta    = JCS({ chain, seq, from_pos, to_pos, from_time, to_time, event_count })
//! head    = H(0x02 || prev_head || root || meta)
//! genesis = H(0x03 || "sthiranet/v1/genesis/" || chain)
//! ```
//!
//! `H` is SHA-256 and JCS is RFC 8785 canonical JSON. Every hash is carried as lowercase hex.
//! This crate is pure: no I/O and no async, so it can also be compiled to WebAssembly.

mod merkle;
mod record;

pub use merkle::{EMPTY_ROOT, MerkleTree, inclusion_proof, merkle_root, merkle_tree, verify_inclusion};
pub use record::{Describe, Description, Record, StoreRow, iso_micros};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Version of the hashing spec implemented here.
pub const SPEC_VERSION: u32 = 1;
/// Salt length in bytes.
pub const SALT_LEN: usize = 32;

/// Domain-separation tags.
pub mod tag {
    pub const LEAF: u8 = 0x00;
    pub const NODE: u8 = 0x01;
    pub const HEAD: u8 = 0x02;
    pub const GENESIS: u8 = 0x03;
}

/// SHA-256 over the concatenation of `parts`, as lowercase hex.
pub fn sha256_hex(parts: &[&[u8]]) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    hex::encode(h.finalize())
}

/// Decodes a hex digest. A value that is not valid hex (for example one somebody edited by hand)
/// is hashed as its raw bytes instead, so it still yields a hash, just a different one.
fn hex_bytes(s: &str) -> Vec<u8> {
    hex::decode(s).unwrap_or_else(|_| s.as_bytes().to_vec())
}

/// RFC 8785 (JCS) canonical JSON of any serialisable value.
///
/// # Panics
/// Only if `v` cannot be represented as JSON (for example a map with non-string keys or a
/// non-finite float). Spec v1 records never contain floats.
pub fn canonical<T: Serialize>(v: &T) -> String {
    serde_json_canonicalizer::to_string(v).expect("value is representable as canonical JSON")
}

/// A fresh random salt (32 bytes) as hex.
pub fn new_salt() -> String {
    let bytes: [u8; SALT_LEN] = rand::random();
    hex::encode(bytes)
}

/// `leaf = H(0x00 || salt || JCS(event))`.
pub fn leaf_hash<T: Serialize>(salt: &str, event: &T) -> String {
    sha256_hex(&[&[tag::LEAF], &hex_bytes(salt), canonical(event).as_bytes()])
}

/// `node = H(0x01 || left || right)`.
pub fn node_hash(left: &str, right: &str) -> String {
    sha256_hex(&[&[tag::NODE], &hex_bytes(left), &hex_bytes(right)])
}

/// Batch metadata that is folded into the chain head. Field names are part of the spec.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BatchMeta {
    pub chain: String,
    pub seq: u64,
    pub from_pos: String,
    pub to_pos: String,
    pub from_time: String,
    pub to_time: String,
    pub event_count: u64,
}

/// `head = H(0x02 || prev_head || root || JCS(meta))`.
pub fn head_hash(prev_head: &str, root: &str, meta: &BatchMeta) -> String {
    sha256_hex(&[&[tag::HEAD], &hex_bytes(prev_head), &hex_bytes(root), canonical(meta).as_bytes()])
}

/// `genesis = H(0x03 || "sthiranet/v1/genesis/" || chain)`.
pub fn genesis_head(chain_id: &str) -> String {
    sha256_hex(&[&[tag::GENESIS], b"sthiranet/v1/genesis/", chain_id.as_bytes()])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn jcs_sorts_keys_and_strips_whitespace() {
        let v = json!({ "b": [1, { "z": null, "a": true }], "a": "x" });
        assert_eq!(canonical(&v), r#"{"a":"x","b":[1,{"a":true,"z":null}]}"#);
    }

    #[test]
    fn jcs_rfc8785_examples() {
        // RFC 8785 §3.2.3: properties are sorted by their UTF-16 code units.
        let v: serde_json::Value = serde_json::from_str(
            r#"{"\u20ac":"Euro Sign","\r":"Carriage Return","\ufb33":"Hebrew Letter Dalet With Dagesh","1":"One","\ud83d\ude00":"Emoji: Grinning Face","\u0080":"Control","\u00f6":"Latin Small Letter O With Diaeresis"}"#,
        )
        .unwrap();
        assert_eq!(
            canonical(&v),
            "{\"\\r\":\"Carriage Return\",\"1\":\"One\",\"\u{80}\":\"Control\",\"ö\":\"Latin Small Letter O With Diaeresis\",\"€\":\"Euro Sign\",\"😀\":\"Emoji: Grinning Face\",\"\u{fb33}\":\"Hebrew Letter Dalet With Dagesh\"}"
        );
        // RFC 8785 §3.2.2.3: number serialisation follows ECMAScript.
        let n: serde_json::Value = serde_json::from_str("[1E30, 4.50, 2e-3, 0.000001, -0]").unwrap();
        assert_eq!(canonical(&n), "[1e+30,4.5,0.002,0.000001,0]");
    }

    #[test]
    fn empty_root_is_hash_of_empty_string() {
        assert_eq!(EMPTY_ROOT, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn salt_is_32_bytes() {
        let s = new_salt();
        assert_eq!(s.len(), SALT_LEN * 2);
        assert_ne!(s, new_salt());
    }

    #[test]
    fn leaf_depends_on_salt_and_content() {
        let salt = "00".repeat(32);
        let a = leaf_hash(&salt, &json!({ "amount": "10.00" }));
        assert_ne!(a, leaf_hash(&salt, &json!({ "amount": "100.00" })));
        assert_ne!(a, leaf_hash(&"01".repeat(32), &json!({ "amount": "10.00" })));
        // Key order does not matter: the event is canonicalised first.
        assert_eq!(leaf_hash(&salt, &json!({ "a": 1, "b": 2 })), leaf_hash(&salt, &json!({ "b": 2, "a": 1 })));
    }

    #[test]
    fn head_chains_previous_head() {
        let meta = BatchMeta {
            chain: "S1".into(),
            seq: 1,
            from_pos: "0/1".into(),
            to_pos: "0/2".into(),
            from_time: "2026-11-09T00:00:00.000000Z".into(),
            to_time: "2026-11-09T00:00:10.000000Z".into(),
            event_count: 0,
        };
        let g = genesis_head("S1");
        assert_ne!(g, genesis_head("S2"));
        let h1 = head_hash(&g, EMPTY_ROOT, &meta);
        assert_ne!(h1, head_hash(&genesis_head("S2"), EMPTY_ROOT, &meta));
        assert_ne!(h1, head_hash(&g, EMPTY_ROOT, &BatchMeta { seq: 2, ..meta.clone() }));
    }
}
