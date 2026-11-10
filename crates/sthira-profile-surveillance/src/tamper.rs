//! Presenter actions: what an insider with access to the recorder and the node's off-ledger store
//! can do AFTER a segment was anchored. Every one of them is caught by recomputation.
//!
//! With `cover`, the insider also uses the camera's own key to make the store look consistent
//! again: the record is re-hashed and re-signed and its stored leaf hash recomputed. The Merkle
//! root the counterparty co-signed cannot be changed that way.

use sthira_core::{StoreRow, leaf_hash};

use crate::{Camera, Clip, Footage, Scene, SegmentRecord, content_hash, synth_bytes};

pub type Rows = Vec<StoreRow<SegmentRecord>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TamperError(pub String);

impl std::fmt::Display for TamperError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TamperError {}

type Result<T> = std::result::Result<T, TamperError>;

fn position(rows: &Rows, seq: u64) -> Result<usize> {
    rows.iter().position(|r| r.record.seq == seq).ok_or_else(|| TamperError(format!("No stored segment #{seq}")))
}

/// Makes the store row agree with whatever bytes are now on disk: new content hash, new device
/// signature, new stored leaf hash.
fn cover_tracks(row: &mut StoreRow<SegmentRecord>, camera: &Camera, bytes: &[u8]) {
    row.record.content_hash = content_hash(bytes);
    camera.sign(&mut row.record);
    row.leaf_hash = leaf_hash(&row.salt, &row.record);
}

const COVERED: &str = " The record was re-hashed and re-signed with the camera's key to cover the tracks.";

/// Splice: replaces the segment's video with different content.
pub fn modify(footage: &mut Footage, rows: &mut Rows, camera: &Camera, seq: u64, cover: bool) -> Result<String> {
    let i = position(rows, seq)?;
    let was = footage.get(seq).map(|c| c.scene);
    let scene = if was == Some(Scene::EmptyAisle) { Scene::Worker } else { Scene::EmptyAisle };
    let bytes = synth_bytes(&camera.info.id, seq, scene, "splice");
    if cover {
        cover_tracks(&mut rows[i], camera, &bytes);
    }
    footage.insert(seq, Clip { scene, bytes });
    Ok(format!(
        "replaced the video of segment #{seq} ({}): it now shows \"{}\".{}",
        rows[i].record.window(),
        scene.label(),
        if cover { COVERED } else { "" }
    ))
}

/// Loop: copies an earlier segment's video over this one.
pub fn replay(footage: &mut Footage, rows: &mut Rows, camera: &Camera, seq: u64, cover: bool) -> Result<String> {
    let i = position(rows, seq)?;
    // Prefer footage from three minutes earlier; otherwise the earliest segment in storage.
    let source = [seq.saturating_sub(18)]
        .into_iter()
        .chain(footage.iter().map(|(s, _)| s))
        .find(|s| *s >= 1 && *s < seq && footage.get(*s).is_some())
        .ok_or_else(|| TamperError(format!("No earlier footage to loop over segment #{seq}")))?;
    let clip = footage.get(source).expect("just found").clone();
    if cover {
        cover_tracks(&mut rows[i], camera, &clip.bytes);
    }
    footage.insert(seq, clip);
    let from = rows.iter().find(|r| r.record.seq == source).map(|r| r.record.window()).unwrap_or_default();
    Ok(format!(
        "looped earlier footage over segment #{seq} ({}): a copy of segment #{source} ({from}).{}",
        rows[i].record.window(),
        if cover { COVERED } else { "" }
    ))
}

/// Deletes one segment: its video and its record. With `cover`, the later segments of the same
/// batch are renumbered and re-signed so that no gap shows in the store.
pub fn delete_segment(
    footage: &mut Footage,
    rows: &mut Rows,
    camera: &Camera,
    seq: u64,
    cover: bool,
) -> Result<String> {
    let i = position(rows, seq)?;
    let gone = rows.remove(i);
    footage.remove(seq);
    let mut renumbered = 0;
    if cover {
        // Each later segment of the batch moves down one slot, taking that slot's number and times.
        let mut slot = (gone.record.seq, gone.index, gone.record.start.clone(), gone.record.end.clone());
        for row in rows.iter_mut().filter(|r| r.batch_seq == gone.batch_seq && r.index > gone.index) {
            let own = (row.record.seq, row.index, row.record.start.clone(), row.record.end.clone());
            let clip = footage.remove(own.0);
            (row.record.seq, row.index, row.record.start, row.record.end) = slot;
            let bytes = clip.as_ref().map(|c| c.bytes.clone()).unwrap_or_default();
            cover_tracks(row, camera, &bytes);
            if let Some(c) = clip {
                footage.insert(row.record.seq, c);
            }
            slot = own;
            renumbered += 1;
        }
    }
    Ok(format!(
        "deleted segment #{seq} ({}).{}",
        gone.record.window(),
        if cover {
            format!(" The next {renumbered} segments were renumbered and re-signed to hide the gap.")
        } else {
            String::new()
        }
    ))
}

/// Deletes every segment of the batch (minute) that segment `seq` belongs to.
pub fn delete_minute(footage: &mut Footage, rows: &mut Rows, seq: u64) -> Result<String> {
    let batch = rows[position(rows, seq)?].batch_seq;
    let gone: Vec<SegmentRecord> = rows.iter().filter(|r| r.batch_seq == batch).map(|r| r.record.clone()).collect();
    rows.retain(|r| r.batch_seq != batch);
    for r in &gone {
        footage.remove(r.seq);
    }
    let hms = |t: &str| t.get(11..19).unwrap_or(t).to_string();
    Ok(format!(
        "deleted a whole minute: {} segments, {}–{}.",
        gone.len(),
        gone.first().map(|r| hms(&r.start)).unwrap_or_default(),
        gone.last().map(|r| hms(&r.end)).unwrap_or_default()
    ))
}
