//! `surveillance/v1` test vectors in `test-vectors/surveillance-v1.json`, produced by an independent
//! Python implementation (`test-vectors/generate.py surveillance`). The camera, the profile and the
//! kernel must together reproduce every value.

use serde::Deserialize;
use sthira_core::{BatchMeta, Record, canonical, genesis_head, head_hash, leaf_hash, merkle_root, verify_inclusion};
use sthira_profile_surveillance::{Camera, Scene, SegmentRecord, content_hash, synth_bytes};

#[derive(Deserialize)]
struct Vectors {
    device: Device,
    segments: Vec<Segment>,
    batch: Batch,
    head: Head,
}

#[derive(Deserialize)]
struct Device {
    seed: String,
    public_key: String,
    start_ms: i64,
}

#[derive(Deserialize)]
struct Segment {
    scene: Scene,
    salt: String,
    record: SegmentRecord,
    signing_payload: String,
    canonical: String,
    leaf: String,
}

#[derive(Deserialize)]
struct Batch {
    leaves: Vec<String>,
    root: String,
    proofs: Vec<Proof>,
}

#[derive(Deserialize)]
struct Proof {
    index: usize,
    path: Vec<String>,
}

#[derive(Deserialize)]
struct Head {
    prev_head: String,
    root: String,
    meta: BatchMeta,
    canonical_meta: String,
    head: String,
}

fn load() -> Vectors {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../test-vectors/surveillance-v1.json");
    serde_json::from_str(&std::fs::read_to_string(path).expect("vector file")).expect("vector json")
}

fn camera(v: &Vectors) -> Camera {
    let seed: [u8; 32] = hex::decode(&v.device.seed).unwrap().try_into().unwrap();
    let c = &v.segments[0].record.camera;
    Camera::new(&c.id, &c.site, &c.zone, seed)
}

#[test]
fn the_camera_reproduces_every_record_and_signature() {
    let v = load();
    let cam = camera(&v);
    assert_eq!(cam.public_key(), v.device.public_key);
    for s in &v.segments {
        let bytes = synth_bytes(&cam.info.id, s.record.seq, s.scene, "capture");
        assert_eq!(content_hash(&bytes), s.record.content_hash);
        let start = v.device.start_ms + (s.record.seq as i64 - 1) * 10_000;
        assert_eq!(cam.record_segment(s.record.seq, start, &bytes), s.record, "segment {}", s.record.seq);
        assert!(s.record.signature_valid(&v.device.public_key));
    }
}

#[test]
fn records_canonicalise_and_hash_to_the_reference_leaves() {
    let v = load();
    for s in &v.segments {
        assert_eq!(s.record.signing_payload(), s.signing_payload);
        assert_eq!(canonical(&s.record), s.canonical);
        assert_eq!(leaf_hash(&s.salt, &s.record), s.leaf);
    }
}

#[test]
fn the_minute_seals_to_the_reference_root_and_head() {
    let v = load();
    let leaves: Vec<String> = v.segments.iter().map(|s| leaf_hash(&s.salt, &s.record)).collect();
    assert_eq!(leaves, v.batch.leaves);
    assert_eq!(merkle_root(&leaves), v.batch.root);
    for p in &v.batch.proofs {
        assert!(verify_inclusion(&leaves[p.index], p.index, leaves.len(), &p.path, &v.batch.root));
    }
    let h = &v.head;
    assert_eq!(h.prev_head, genesis_head(v.segments[0].record.chain()));
    assert_eq!(
        (h.meta.from_pos.clone(), h.meta.to_pos.clone()),
        (v.segments[0].record.pos(), v.segments[5].record.pos())
    );
    assert_eq!(canonical(&h.meta), h.canonical_meta);
    assert_eq!(head_hash(&h.prev_head, &h.root, &h.meta), h.head);
}
