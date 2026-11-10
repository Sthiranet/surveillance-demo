//! Connector layer · surveillance record profile (`surveillance/v1`).
//!
//! A camera cuts its video into 10-second segments. For each one it fingerprints the bytes, builds
//! a [`SegmentRecord`] and signs it with its device key. The record, never the video, is what the
//! node layer hashes into a leaf and anchors.
//!
//! ```text
//! record       = { v, profile, chain, camera{id, site, zone, key_id}, seq, start, end,
//!                  media{codec, fps, frames, bytes}, content_hash, device_sig }
//! content_hash = SHA-256(segment bytes)
//! device_sig   = Ed25519 over JCS(record without device_sig)
//! leaf         = H(0x00 || salt || JCS(record))          (hashing spec v1, unchanged)
//! ```
//!
//! 1. [`Camera`] stands in for an edge camera with a C2PA-compatible signer.
//! 2. [`Footage`] stands in for the recorder (NVR): the segment bytes, which never leave their owner.
//!    The bytes are synthetic ([`synth_bytes`]); [`SceneScript`] decides what each segment "shows".
//! 3. [`checks`] are the profile's own verification rules: content hash, signature, contiguity, replay.
//! 4. [`tamper`] is what an insider with access to the recorder can do after the fact.
//!
//! The kernel and node layers see this crate only through `sthira_core::{Record, Describe}`.

mod camera;
pub mod checks;
mod footage;
mod record;
pub mod tamper;

pub use camera::Camera;
pub use checks::{Gap, SegmentCheck, SegmentStatus, check_segments, gaps};
pub use footage::{Clip, Footage, Scene, SceneScript, synth_bytes};
pub use record::{CameraInfo, Media, SegmentRecord};

/// Profile label carried by every record and by the chain's spec.
pub const PROFILE: &str = "surveillance/v1";
/// Contract template label of a camera chain's anchors.
pub const TEMPLATE: &str = "Sthiranet.Surveillance:SegmentAnchor";
/// Length of one segment.
pub const SEGMENT_MS: i64 = 10_000;
/// Size of a synthetic segment (the demo's stand-in for encoded video).
pub const SEGMENT_BYTES: usize = 4096;

/// SHA-256 of a segment's bytes, as lowercase hex.
pub fn content_hash(bytes: &[u8]) -> String {
    sthira_core::sha256_hex(&[bytes])
}
