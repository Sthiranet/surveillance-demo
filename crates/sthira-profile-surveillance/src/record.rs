//! The signed segment record: the exact structure that is canonicalised and hashed into a leaf.
//! Field names and shapes are part of the `surveillance/v1` profile.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sthira_core::{Describe, Description, Record, canonical};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CameraInfo {
    pub id: String,
    pub site: String,
    pub zone: String,
    /// Short identifier of the device key that signs this camera's records.
    pub key_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Media {
    pub codec: String,
    pub fps: u32,
    pub frames: u32,
    pub bytes: u64,
}

/// One 10-second segment, as the camera attests it.
///
/// - `seq` counts 1, 2, 3… per camera and is never reused.
/// - `start` and `end` are RFC 3339 UTC with microseconds; `end - start` is 10 s.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SegmentRecord {
    pub v: u32,
    pub profile: String,
    pub chain: String,
    pub camera: CameraInfo,
    pub seq: u64,
    pub start: String,
    pub end: String,
    pub media: Media,
    /// SHA-256 of the segment bytes, hex.
    pub content_hash: String,
    /// Ed25519 over the canonical record without this field, hex.
    pub device_sig: String,
}

impl SegmentRecord {
    /// What the device signs: `JCS(record without device_sig)`.
    pub fn signing_payload(&self) -> String {
        let mut v = serde_json::to_value(self).expect("a segment record is plain JSON");
        if let Some(o) = v.as_object_mut() {
            o.remove("device_sig");
        }
        canonical(&v)
    }

    /// Whether `device_sig` is this record signed by the device key `public_key` (hex).
    pub fn signature_valid(&self, public_key: &str) -> bool {
        let key = hex::decode(public_key).ok().and_then(|b| <[u8; 32]>::try_from(b).ok());
        let sig = hex::decode(&self.device_sig).ok().and_then(|b| <[u8; 64]>::try_from(b).ok());
        let (Some(key), Some(sig)) = (key, sig) else { return false };
        let Ok(key) = VerifyingKey::from_bytes(&key) else { return false };
        key.verify(self.signing_payload().as_bytes(), &Signature::from_bytes(&sig)).is_ok()
    }

    /// `08:21:10–08:21:20`, from the record's own timestamps.
    pub fn window(&self) -> String {
        let hms = |t: &str| t.get(11..19).unwrap_or(t).to_string();
        format!("{}–{}", hms(&self.start), hms(&self.end))
    }
}

impl Record for SegmentRecord {
    fn chain(&self) -> &str {
        &self.chain
    }

    /// The segment number, zero-padded so positions sort as text.
    fn pos(&self) -> String {
        format!("{:010}", self.seq)
    }
}

impl Describe for SegmentRecord {
    fn describe(&self) -> Description {
        Description {
            kind: "segment".into(),
            label: format!("Segment #{}", self.seq),
            text: format!("{} · {} · {}", self.camera.id, self.window(), self.camera.zone),
        }
    }
}
