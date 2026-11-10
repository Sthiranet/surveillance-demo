//! RFC 6962 Merkle tree hash over leaf hashes, kept as a whole tree so a UI can draw it,
//! plus inclusion (audit) proofs.
//!
//! For `n > 1` leaves the tree splits at `k`, the largest power of two strictly below `n`:
//! `MTH(D[0:n]) = node(MTH(D[0:k]), MTH(D[k:n]))`.

use serde::{Deserialize, Serialize};

use crate::node_hash;

/// `H("")`: the root of an empty batch.
pub const EMPTY_ROOT: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

/// A Merkle tree node. A leaf has `leaf` (its index in the batch); an inner node has `left` and
/// `right`; the tree of an empty batch is a single node with `empty: true`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct MerkleTree {
    pub hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub leaf: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(no_recursion)]
    pub left: Option<Box<MerkleTree>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schema(no_recursion)]
    pub right: Option<Box<MerkleTree>>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub empty: bool,
}

/// Largest power of two strictly below `n` (`n >= 2`).
fn split(n: usize) -> usize {
    debug_assert!(n >= 2);
    let mut k = 1;
    while k * 2 < n {
        k *= 2;
    }
    k
}

/// Builds the whole tree over `leaves` (leaf hashes, in batch order).
pub fn merkle_tree(leaves: &[String]) -> MerkleTree {
    fn build(leaves: &[String], lo: usize, hi: usize) -> MerkleTree {
        let n = hi - lo;
        if n == 1 {
            return MerkleTree { hash: leaves[lo].clone(), leaf: Some(lo), left: None, right: None, empty: false };
        }
        let k = split(n);
        let left = build(leaves, lo, lo + k);
        let right = build(leaves, lo + k, hi);
        MerkleTree {
            hash: node_hash(&left.hash, &right.hash),
            leaf: None,
            left: Some(Box::new(left)),
            right: Some(Box::new(right)),
            empty: false,
        }
    }
    if leaves.is_empty() {
        return MerkleTree { hash: EMPTY_ROOT.to_string(), leaf: None, left: None, right: None, empty: true };
    }
    build(leaves, 0, leaves.len())
}

/// The Merkle root alone.
pub fn merkle_root(leaves: &[String]) -> String {
    fn mth(leaves: &[String]) -> String {
        match leaves.len() {
            1 => leaves[0].clone(),
            n => {
                let k = split(n);
                node_hash(&mth(&leaves[..k]), &mth(&leaves[k..]))
            }
        }
    }
    if leaves.is_empty() { EMPTY_ROOT.to_string() } else { mth(leaves) }
}

/// RFC 6962 audit path for leaf `index`: sibling hashes ordered from the leaf up to the root.
/// Returns `None` if `index` is out of range.
pub fn inclusion_proof(leaves: &[String], index: usize) -> Option<Vec<String>> {
    fn path(leaves: &[String], m: usize, out: &mut Vec<String>) {
        let n = leaves.len();
        if n == 1 {
            return;
        }
        let k = split(n);
        if m < k {
            path(&leaves[..k], m, out);
            out.push(merkle_root(&leaves[k..]));
        } else {
            path(&leaves[k..], m - k, out);
            out.push(merkle_root(&leaves[..k]));
        }
    }
    if index >= leaves.len() {
        return None;
    }
    let mut out = Vec::new();
    path(leaves, index, &mut out);
    Some(out)
}

/// Checks that `leaf` sits at `index` in a tree of `size` leaves whose root is `root`.
pub fn verify_inclusion(leaf: &str, index: usize, size: usize, proof: &[String], root: &str) -> bool {
    fn fold(hash: &str, m: usize, n: usize, proof: &[String]) -> Option<String> {
        if n == 1 {
            return proof.is_empty().then(|| hash.to_string());
        }
        let (sibling, rest) = proof.split_last()?;
        let k = split(n);
        if m < k {
            Some(node_hash(&fold(hash, m, k, rest)?, sibling))
        } else {
            Some(node_hash(sibling, &fold(hash, m - k, n - k, rest)?))
        }
    }
    index < size && fold(leaf, index, size, proof).is_some_and(|r| r == root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{sha256_hex, tag};
    use proptest::prelude::*;

    /// Leaf hashes of the RFC 6962 reference inputs (certificate-transparency test data).
    fn ct_leaves() -> Vec<String> {
        let inputs: [&[u8]; 8] = [
            b"",
            b"\x00",
            b"\x10",
            b"\x20\x21",
            b"\x30\x31",
            b"\x40\x41\x42\x43",
            b"\x50\x51\x52\x53\x54\x55\x56\x57",
            b"\x60\x61\x62\x63\x64\x65\x66\x67\x68\x69\x6a\x6b\x6c\x6d\x6e\x6f",
        ];
        inputs.iter().map(|d| sha256_hex(&[&[tag::LEAF], d])).collect()
    }

    #[test]
    fn rfc6962_reference_roots() {
        let expected = [
            "6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d",
            "fac54203e7cc696cf0dfcb42c92a1d9dbaf70ad9e621f4bd8d98662f00e3c125",
            "aeb6bcfe274b70a14fb067a5e5578264db0fa9b51af5e0ba159158f329e06e77",
            "d37ee418976dd95753c1c73862b9398fa2a2cf9b4ff0fdfe8b30cd95209614b7",
            "4e3bbb1f7b478dcfe71fb631631519a3bca12c9aefca1612bfce4c13a86264d4",
            "76e67dadbcdf1e10e1b74ddc608abd2f98dfb16fbce75277b5232a127f2087ef",
            "ddb89be403809e325750d3d263cd78929c2942b7942a34b77e122c9594a74c8c",
            "5dc9da79a70659a9ad559cb701ded9a2ab9d823aad2f4960cfe370eff4604328",
        ];
        let leaves = ct_leaves();
        for (n, want) in expected.iter().enumerate() {
            let sub = &leaves[..=n];
            assert_eq!(merkle_root(sub), *want, "root of {} leaves", n + 1);
            assert_eq!(merkle_tree(sub).hash, *want, "tree of {} leaves", n + 1);
        }
    }

    #[test]
    fn tree_shape() {
        let leaves = ct_leaves();
        let t = merkle_tree(&leaves[..3]);
        // 3 leaves split 2 | 1.
        assert_eq!(t.right.as_ref().unwrap().leaf, Some(2));
        assert_eq!(t.left.as_ref().unwrap().left.as_ref().unwrap().leaf, Some(0));
        let e = merkle_tree(&[]);
        assert!(e.empty);
        assert_eq!(e.hash, EMPTY_ROOT);
        let json = serde_json::to_value(merkle_tree(&leaves[..1])).unwrap();
        assert_eq!(json, serde_json::json!({ "hash": leaves[0], "leaf": 0 }));
    }

    fn arb_leaves() -> impl Strategy<Value = Vec<String>> {
        prop::collection::vec(any::<[u8; 32]>().prop_map(hex::encode), 1..40)
    }

    proptest! {
        #[test]
        fn every_valid_proof_verifies(leaves in arb_leaves(), pick in any::<prop::sample::Index>()) {
            let i = pick.index(leaves.len());
            let root = merkle_root(&leaves);
            let proof = inclusion_proof(&leaves, i).unwrap();
            prop_assert!(verify_inclusion(&leaves[i], i, leaves.len(), &proof, &root));
        }

        #[test]
        fn any_flipped_byte_fails(
            leaves in arb_leaves(),
            pick in any::<prop::sample::Index>(),
            target in any::<prop::sample::Index>(),
            byte in 0usize..32,
            bit in 0u8..8,
        ) {
            let i = pick.index(leaves.len());
            let root = merkle_root(&leaves);
            let proof = inclusion_proof(&leaves, i).unwrap();
            let flip = |h: &str| {
                let mut b = hex::decode(h).unwrap();
                b[byte] ^= 1 << bit;
                hex::encode(b)
            };
            // Flip the leaf, one proof element, or the root: each must fail.
            let slots = proof.len() + 2;
            match target.index(slots) {
                0 => prop_assert!(!verify_inclusion(&flip(&leaves[i]), i, leaves.len(), &proof, &root)),
                1 => prop_assert!(!verify_inclusion(&leaves[i], i, leaves.len(), &proof, &flip(&root))),
                s => {
                    let mut p = proof.clone();
                    p[s - 2] = flip(&p[s - 2]);
                    prop_assert!(!verify_inclusion(&leaves[i], i, leaves.len(), &p, &root));
                }
            }
        }
    }

    #[test]
    fn proof_rejects_wrong_index_or_size() {
        let leaves = ct_leaves();
        let root = merkle_root(&leaves);
        let proof = inclusion_proof(&leaves, 5).unwrap();
        assert!(verify_inclusion(&leaves[5], 5, 8, &proof, &root));
        assert!(!verify_inclusion(&leaves[5], 4, 8, &proof, &root));
        // A different tree size gives a different path shape (8 leaves: 3 siblings; 6 leaves: 2).
        assert!(!verify_inclusion(&leaves[5], 5, 6, &proof, &root));
        assert!(inclusion_proof(&leaves, 8).is_none());
    }
}
