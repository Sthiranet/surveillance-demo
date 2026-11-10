//! The tamper matrix: every attack in the build plan is detected, and untouched storage is clean.
//!
//! "Detected" here uses only what a verifier has: the records and video in storage, the device key
//! agreed at genesis, and per batch the Merkle root and segment count that were anchored.

use proptest::prelude::*;
use sthira_core::{StoreRow, leaf_hash, merkle_root};
use sthira_profile_surveillance::tamper::{self, Rows};
use sthira_profile_surveillance::{
    Camera, Clip, Footage, SceneScript, SegmentRecord, SegmentStatus, check_segments, gaps, synth_bytes,
};

const START_MS: i64 = 1_794_297_600_000; // 2026-11-10T08:00:00Z
const PER_BATCH: u64 = 6;

struct Site {
    camera: Camera,
    footage: Footage,
    rows: Rows,
    /// What was anchored: per batch, (Merkle root, segment count).
    anchored: Vec<(String, usize)>,
}

fn site(batches: u64) -> Site {
    let camera = Camera::new("CAM-CAGE", "WH-PUNE-01", "cage", [9u8; 32]);
    let (mut footage, mut rows, mut anchored) = (Footage::new(), Vec::new(), Vec::new());
    for b in 1..=batches {
        let mut leaves = Vec::new();
        for index in 0..PER_BATCH as usize {
            let seq = (b - 1) * PER_BATCH + index as u64 + 1;
            let scene = SceneScript::scene("cage", seq);
            let bytes = synth_bytes("CAM-CAGE", seq, scene, "capture");
            let record = camera.record_segment(seq, START_MS + (seq as i64 - 1) * 10_000, &bytes);
            footage.insert(seq, Clip { scene, bytes });
            let salt = format!("{:02x}", seq % 251).repeat(32);
            let leaf = leaf_hash(&salt, &record);
            leaves.push(leaf.clone());
            rows.push(StoreRow {
                id: format!("CAM-CAGE-{seq:05}"),
                chain: "CAM-CAGE".into(),
                batch_seq: b,
                index,
                record,
                salt,
                leaf_hash: leaf,
            });
        }
        anchored.push((merkle_root(&leaves), leaves.len()));
    }
    Site { camera, footage, rows, anchored }
}

/// What a verifier finds wrong, by kind.
#[derive(Debug, Default, PartialEq, Eq)]
struct Findings {
    bad_content: Vec<u64>,
    replayed: Vec<u64>,
    bad_signature: Vec<u64>,
    root_mismatch: Vec<u64>,
    count_mismatch: Vec<u64>,
    gaps: usize,
}

impl Findings {
    fn clean(&self) -> bool {
        *self == Self::default()
    }
}

fn verify(s: &Site) -> Findings {
    let mut f = Findings::default();
    let records: Vec<&SegmentRecord> = s.rows.iter().map(|r| &r.record).collect();
    for c in check_segments(records.iter().copied(), &s.footage, &s.camera.public_key()) {
        match c.status {
            SegmentStatus::Ok => {}
            SegmentStatus::Replayed => f.replayed.push(c.seq),
            SegmentStatus::BadSignature => f.bad_signature.push(c.seq),
            SegmentStatus::Modified | SegmentStatus::Missing => f.bad_content.push(c.seq),
        }
    }
    f.gaps = gaps(records.iter().copied()).len();
    for (i, (root, count)) in s.anchored.iter().enumerate() {
        let b = i as u64 + 1;
        let mut batch: Vec<&StoreRow<SegmentRecord>> = s.rows.iter().filter(|r| r.batch_seq == b).collect();
        batch.sort_by_key(|r| r.index);
        let leaves: Vec<String> = batch.iter().map(|r| leaf_hash(&r.salt, &r.record)).collect();
        if merkle_root(&leaves) != *root {
            f.root_mismatch.push(b);
        }
        if leaves.len() != *count {
            f.count_mismatch.push(b);
        }
    }
    f
}

#[test]
fn untouched_storage_is_clean() {
    assert!(verify(&site(5)).clean());
}

#[test]
fn a_spliced_segment_fails_its_content_hash() {
    let mut s = site(4);
    let msg = tamper::modify(&mut s.footage, &mut s.rows, &s.camera, 9, false).unwrap();
    assert!(msg.contains("segment #9"), "{msg}");
    let f = verify(&s);
    assert_eq!(f.bad_content, [9]);
    assert!(f.root_mismatch.is_empty(), "the record is untouched; only the video changed");
}

#[test]
fn covered_tracks_pass_the_camera_checks_but_not_the_anchored_root() {
    let mut s = site(4);
    tamper::modify(&mut s.footage, &mut s.rows, &s.camera, 9, true).unwrap();
    let f = verify(&s);
    // Re-hashed and re-signed with the camera's own key: hash and signature now agree…
    assert!(f.bad_content.is_empty() && f.bad_signature.is_empty());
    let row = s.rows.iter().find(|r| r.record.seq == 9).unwrap();
    assert_eq!(row.leaf_hash, leaf_hash(&row.salt, &row.record), "the stored leaf looks consistent too");
    // …but the root the counterparty co-signed is out of the insider's reach.
    assert_eq!(f.root_mismatch, [2]);
}

#[test]
fn looped_footage_is_recognised_as_a_copy() {
    let mut s = site(5);
    let msg = tamper::replay(&mut s.footage, &mut s.rows, &s.camera, 25, false).unwrap();
    assert!(msg.contains("copy of segment #7"), "{msg}");
    assert_eq!(verify(&s).replayed, [25]);

    // Even re-signed, the bytes are still a duplicate of earlier footage, and the root moves.
    let mut s = site(5);
    tamper::replay(&mut s.footage, &mut s.rows, &s.camera, 25, true).unwrap();
    let f = verify(&s);
    assert_eq!((f.replayed, f.root_mismatch), (vec![25], vec![5]));

    assert!(tamper::replay(&mut s.footage, &mut s.rows, &s.camera, 1, false).is_err(), "nothing earlier to loop");
}

#[test]
fn a_deleted_segment_leaves_a_gap_and_a_short_batch() {
    let mut s = site(4);
    tamper::delete_segment(&mut s.footage, &mut s.rows, &s.camera, 14, false).unwrap();
    let f = verify(&s);
    assert_eq!((f.count_mismatch, f.root_mismatch, f.gaps), (vec![3], vec![3], 1));
}

#[test]
fn renumbering_around_a_gap_does_not_hide_it() {
    let mut s = site(4);
    let msg = tamper::delete_segment(&mut s.footage, &mut s.rows, &s.camera, 14, true).unwrap();
    assert!(msg.contains("next 4 segments"), "{msg}");
    let f = verify(&s);
    // Inside the batch the numbering is seamless again and every signature and hash checks out…
    assert!(f.bad_content.is_empty() && f.bad_signature.is_empty());
    let batch3: Vec<u64> = s.rows.iter().filter(|r| r.batch_seq == 3).map(|r| r.record.seq).collect();
    assert_eq!(batch3, [13, 14, 15, 16, 17]);
    // …but the anchor still says six segments under a different root.
    assert_eq!((f.count_mismatch, f.root_mismatch), (vec![3], vec![3]));
}

#[test]
fn a_deleted_minute_is_an_anchored_batch_with_nothing_in_it() {
    let mut s = site(4);
    let msg = tamper::delete_minute(&mut s.footage, &mut s.rows, 8).unwrap();
    assert!(msg.contains("6 segments, 08:01:00–08:02:00"), "{msg}");
    let f = verify(&s);
    assert_eq!((f.count_mismatch, f.root_mismatch, f.gaps), (vec![2], vec![2], 1));
    assert_eq!(s.footage.len(), 18);
}

#[test]
fn a_record_edited_without_the_device_key_fails_its_signature() {
    let mut s = site(2);
    s.rows[3].record.start = "2026-11-10T09:00:00.000000Z".into();
    let f = verify(&s);
    assert_eq!(f.bad_signature, [4]);
    // A signature from some other key does not help either.
    let other = Camera::new("CAM-CAGE", "WH-PUNE-01", "cage", [1u8; 32]);
    other.sign(&mut s.rows[3].record);
    assert_eq!(verify(&s).bad_signature, [4]);
}

#[test]
fn unknown_segments_are_refused() {
    let mut s = site(1);
    assert!(tamper::modify(&mut s.footage, &mut s.rows, &s.camera, 99, false).is_err());
    assert!(tamper::delete_minute(&mut s.footage, &mut s.rows, 99).is_err());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn any_single_byte_change_in_any_segment_is_detected(seq in 1u64..=24, at in 0usize..4096, bit in 0u8..8) {
        let mut s = site(4);
        s.footage.get_mut(seq).unwrap().bytes[at] ^= 1 << bit;
        prop_assert_eq!(verify(&s).bad_content, vec![seq]);
    }

    #[test]
    fn any_deletion_is_detected(seq in 1u64..=24, cover in any::<bool>(), minute in any::<bool>()) {
        let mut s = site(4);
        if minute {
            tamper::delete_minute(&mut s.footage, &mut s.rows, seq).unwrap();
        } else {
            tamper::delete_segment(&mut s.footage, &mut s.rows, &s.camera, seq, cover).unwrap();
        }
        let batch = (seq - 1) / PER_BATCH + 1;
        let f = verify(&s);
        prop_assert_eq!(f.count_mismatch, vec![batch]);
        prop_assert_eq!(f.root_mismatch, vec![batch]);
    }

    #[test]
    fn covered_tracks_are_always_caught_at_the_root(seq in 2u64..=24, looped in any::<bool>()) {
        let mut s = site(4);
        if looped {
            tamper::replay(&mut s.footage, &mut s.rows, &s.camera, seq, true).unwrap();
        } else {
            tamper::modify(&mut s.footage, &mut s.rows, &s.camera, seq, true).unwrap();
        }
        prop_assert_eq!(verify(&s).root_mismatch, vec![(seq - 1) / PER_BATCH + 1]);
    }
}
