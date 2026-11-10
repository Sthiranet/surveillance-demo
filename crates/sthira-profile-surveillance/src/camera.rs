//! An edge camera: it holds the device key and attests each segment it records.

use ed25519_dalek::{Signer, SigningKey};
use sthira_core::{SPEC_VERSION, iso_micros};

use crate::{CameraInfo, Media, PROFILE, SEGMENT_MS, SegmentRecord, content_hash};

/// Stands in for a camera with a hardware-held, C2PA-compatible signing key. Each camera feeds one
/// chain, named after the camera.
pub struct Camera {
    pub info: CameraInfo,
    key: SigningKey,
}

impl Camera {
    /// A camera whose device key is derived from `seed` (deterministic, for tests and vectors).
    pub fn new(id: &str, site: &str, zone: &str, seed: [u8; 32]) -> Self {
        let key = SigningKey::from_bytes(&seed);
        let public = hex::encode(key.verifying_key().to_bytes());
        Self {
            info: CameraInfo {
                id: id.into(),
                site: site.into(),
                zone: zone.into(),
                key_id: format!("ed25519:{}", &public[..16]),
            },
            key,
        }
    }

    /// A camera with a fresh random device key.
    pub fn generate(id: &str, site: &str, zone: &str) -> Self {
        Self::new(id, site, zone, rand::random())
    }

    /// The device's public key (hex). Registered in the chain's attributes at genesis.
    pub fn public_key(&self) -> String {
        hex::encode(self.key.verifying_key().to_bytes())
    }

    /// Fingerprints and signs segment `seq`, which starts at `start_ms` (Unix milliseconds).
    pub fn record_segment(&self, seq: u64, start_ms: i64, bytes: &[u8]) -> SegmentRecord {
        let mut record = SegmentRecord {
            v: SPEC_VERSION,
            profile: PROFILE.into(),
            chain: self.info.id.clone(),
            camera: self.info.clone(),
            seq,
            start: iso_micros(start_ms),
            end: iso_micros(start_ms + SEGMENT_MS),
            media: Media { codec: "h265".into(), fps: 25, frames: 250, bytes: bytes.len() as u64 },
            content_hash: content_hash(bytes),
            device_sig: String::new(),
        };
        self.sign(&mut record);
        record
    }

    /// (Re)signs `record` with the device key.
    pub fn sign(&self, record: &mut SegmentRecord) {
        record.device_sig = hex::encode(self.key.sign(record.signing_payload().as_bytes()).to_bytes());
    }
}
