//! The recorder (NVR): where the segment bytes live. In the demo the bytes are synthetic and a
//! scene tag says what the segment would show.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{SEGMENT_BYTES, content_hash};

/// What a segment shows. Illustration only: the scene is not part of the signed record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Scene {
    TruckDocking,
    PalletUnloading,
    Forklift,
    Worker,
    EmptyAisle,
}

impl Scene {
    pub fn tag(self) -> &'static str {
        match self {
            Self::TruckDocking => "truck_docking",
            Self::PalletUnloading => "pallet_unloading",
            Self::Forklift => "forklift",
            Self::Worker => "worker",
            Self::EmptyAisle => "empty_aisle",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::TruckDocking => "truck docking",
            Self::PalletUnloading => "pallets being unloaded",
            Self::Forklift => "forklift moving pallets",
            Self::Worker => "worker in view",
            Self::EmptyAisle => "empty aisle",
        }
    }
}

/// The deterministic storyline: what each camera zone shows in each segment.
///
/// The first 30 minutes (segments 1–180) end with the consignment's story, told in the ten minutes
/// from 08:20 (segments 121–180): a truck docks, pallets are unloaded, a forklift carries them to
/// the high-value cage, and a dispatch leaves at the end. Before that the aisles are quiet; after
/// it the zones cycle through ordinary activity.
pub struct SceneScript;

impl SceneScript {
    pub fn scene(zone: &str, seq: u64) -> Scene {
        use Scene::*;
        let (story, offset) = match zone {
            "dock" => (
                match seq {
                    121..=132 => Some(TruckDocking),
                    133..=150 => Some(PalletUnloading),
                    151..=156 => Some(Forklift),
                    157..=162 => Some(Worker),
                    _ => None,
                },
                0,
            ),
            "cage" => (
                match seq {
                    151..=162 => Some(Forklift),
                    163..=174 => Some(Worker),
                    _ => None,
                },
                7,
            ),
            _ => (
                match seq {
                    139..=144 => Some(Worker),
                    163..=174 => Some(TruckDocking),
                    175..=180 => Some(PalletUnloading),
                    _ => None,
                },
                13,
            ),
        };
        if let Some(s) = story {
            return s;
        }
        if seq <= 180 {
            // Background: an empty aisle, with someone walking through now and then.
            return if (seq + offset).is_multiple_of(23) { Worker } else { EmptyAisle };
        }
        match ((seq - 181) / 6 + offset) % 10 {
            0 if zone == "cage" => Forklift,
            0 => TruckDocking,
            1 | 2 if zone == "cage" => Worker,
            1 | 2 => PalletUnloading,
            3 => Forklift,
            4 => Worker,
            _ => EmptyAisle,
        }
    }
}

/// Deterministic stand-in for a segment's encoded video: [`SEGMENT_BYTES`] bytes of SHA-256 output
/// in counter mode over `(camera, seq, scene, variant)`. `variant` separates what the camera
/// captured (`"capture"`) from what somebody spliced in later.
pub fn synth_bytes(camera: &str, seq: u64, scene: Scene, variant: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(SEGMENT_BYTES);
    let mut block = 0u32;
    while out.len() < SEGMENT_BYTES {
        let seed = format!("sthiranet/surveillance/v1/synthetic|{camera}|{seq}|{}|{variant}|{block}", scene.tag());
        out.extend_from_slice(&Sha256::digest(seed.as_bytes()));
        block += 1;
    }
    out.truncate(SEGMENT_BYTES);
    out
}

/// One stored segment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clip {
    pub scene: Scene,
    pub bytes: Vec<u8>,
}

/// One camera's stored segments, by segment number.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Footage {
    clips: BTreeMap<u64, Clip>,
}

impl Footage {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, seq: u64, clip: Clip) {
        self.clips.insert(seq, clip);
    }

    pub fn get(&self, seq: u64) -> Option<&Clip> {
        self.clips.get(&seq)
    }

    pub fn get_mut(&mut self, seq: u64) -> Option<&mut Clip> {
        self.clips.get_mut(&seq)
    }

    pub fn remove(&mut self, seq: u64) -> Option<Clip> {
        self.clips.remove(&seq)
    }

    pub fn len(&self) -> usize {
        self.clips.len()
    }

    pub fn is_empty(&self) -> bool {
        self.clips.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (u64, &Clip)> {
        self.clips.iter().map(|(s, c)| (*s, c))
    }

    /// SHA-256 of the bytes currently stored for `seq`.
    pub fn hash(&self, seq: u64) -> Option<String> {
        self.get(seq).map(|c| content_hash(&c.bytes))
    }

    /// The clips of the given segments only: what is handed over as evidence for a window.
    pub fn excerpt(&self, seqs: impl IntoIterator<Item = u64>) -> Self {
        Self { clips: seqs.into_iter().filter_map(|s| self.clips.get(&s).map(|c| (s, c.clone()))).collect() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_bytes_are_deterministic_and_distinct() {
        let a = synth_bytes("CAM-DOCK", 7, Scene::EmptyAisle, "capture");
        assert_eq!(a.len(), SEGMENT_BYTES);
        assert_eq!(a, synth_bytes("CAM-DOCK", 7, Scene::EmptyAisle, "capture"));
        assert_ne!(a, synth_bytes("CAM-DOCK", 8, Scene::EmptyAisle, "capture"));
        assert_ne!(a, synth_bytes("CAM-CAGE", 7, Scene::EmptyAisle, "capture"));
        assert_ne!(a, synth_bytes("CAM-DOCK", 7, Scene::Worker, "capture"));
        assert_ne!(a, synth_bytes("CAM-DOCK", 7, Scene::EmptyAisle, "splice"));
    }

    #[test]
    fn the_storyline_puts_the_delivery_on_the_dock_then_in_the_cage() {
        assert_eq!(SceneScript::scene("dock", 121), Scene::TruckDocking);
        assert_eq!(SceneScript::scene("dock", 140), Scene::PalletUnloading);
        assert_eq!(SceneScript::scene("cage", 155), Scene::Forklift);
        assert_eq!(SceneScript::scene("cage", 5), Scene::EmptyAisle);
        assert_eq!(SceneScript::scene("dock", 61), Scene::EmptyAisle);
        assert_eq!(SceneScript::scene("dispatch", 170), Scene::TruckDocking);
        // Deterministic for any segment number.
        assert_eq!(SceneScript::scene("dock", 9_999), SceneScript::scene("dock", 9_999));
    }
}
