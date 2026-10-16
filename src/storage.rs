//! Storage budget (SPEC 6, Storage): keeps the crate under `storage.max_gb` by deleting the least
//! recently played tracks that are not kept, and reports when only kept tracks are left.

use serde_json::{Value, json};

use crate::{config::StoreFormat, state::AppState, store::Store};

pub fn max_bytes(state: &AppState) -> Option<i64> {
    let gb = state.cfg.storage.max_gb;
    (gb > 0.0).then(|| (gb * 1e9) as i64)
}

/// Likely size of a new track of `seconds`, with 10% headroom.
pub fn estimate_bytes(state: &AppState, seconds: u32) -> i64 {
    let per_second = match state.cfg.storage.format {
        StoreFormat::Flac => 48_000 * 2 * 2 / 2, // about half of raw 48 kHz stereo PCM
        _ => i64::from(state.cfg.storage.bitrate_k) * 1000 / 8,
    };
    i64::from(seconds) * per_second * 11 / 10
}

/// Deletes tracks in eviction order until `needed` more bytes fit; true when they do.
/// Kept, queued and playing tracks, and those in `protect`, are never deleted.
pub fn make_room(state: &AppState, needed: i64, protect: &[String]) -> bool {
    let Some(max) = max_bytes(state) else { return true };
    let mut used = state.store.usage().unwrap_or(0);
    if used + needed <= max {
        return true;
    }
    let mut keep = state.busy_track_ids();
    keep.extend_from_slice(protect);
    for (id, file, bytes) in state.store.eviction_order(&keep).unwrap_or_default() {
        let _ = std::fs::remove_file(&file);
        if state.store.mark_deleted(&id).is_ok() {
            used -= bytes;
            tracing::info!(track = %id, bytes, "deleted to stay within the storage budget");
        }
        if used + needed <= max {
            return true;
        }
    }
    false
}

/// For `GET /status`.
pub fn status(state: &AppState) -> Value {
    let used = state.store.usage().unwrap_or(0);
    let max = max_bytes(state);
    json!({
        "format": state.cfg.storage.format,
        "used_bytes": used,
        "max_bytes": max,
        "full": max.is_some_and(|m| used >= m),
    })
}

/// Fills in sizes for tracks stored before sizes were recorded.
pub fn backfill(store: &Store) {
    for (id, file) in store.tracks_without_bytes().unwrap_or_default() {
        if let Ok(meta) = std::fs::metadata(&file) {
            let _ = store.set_bytes(&id, meta.len() as i64);
        }
    }
}
