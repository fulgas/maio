//! One live radio's runtime (SPEC 10): its queue, playout, PCM bus, pull buses and outputs.

use std::sync::{Arc, Mutex, atomic::AtomicUsize};

use bytes::Bytes;
use tokio::sync::{broadcast, watch};

use crate::{
    outputs, playout,
    queue::Queue,
    state::{AppState, Backoff, OutputStatus, Playout},
    store::{RadioRow, Visibility},
};

pub struct Radio {
    pub id: String,
    pub row: Mutex<RadioRow>,
    pub queue: Mutex<Queue>,
    pub playout: Mutex<Playout>,
    pub outputs: Mutex<Vec<OutputStatus>>,
    pub backoff: Mutex<Backoff>,
    /// The play order drawn for the playlist on air (SPEC 10, On air).
    pub play_order: Mutex<playout::PlayOrder>,
    /// 20 ms PCM chunks from this radio's playout; its outputs subscribe.
    pub pcm: broadcast::Sender<Bytes>,
    /// MP3 bytes for this radio's pull listeners.
    pub mp3: broadcast::Sender<Bytes>,
    /// AAC (ADTS) bytes for this radio's pull listeners.
    pub aac: broadcast::Sender<Bytes>,
    /// Open pull-stream connections (SPEC 10, listener count).
    pub listeners: AtomicUsize,
    /// Bumped when the visibility or listen key changes; open pull streams end on a bump.
    pub access: watch::Sender<u64>,
    /// True once the radio is deleted: its playout and encoders stop.
    pub closed: watch::Sender<bool>,
}

impl Radio {
    pub fn new(row: RadioRow) -> Arc<Self> {
        Arc::new(Self {
            id: row.id.clone(),
            row: Mutex::new(row),
            queue: Mutex::default(),
            playout: Mutex::default(),
            outputs: Mutex::default(),
            backoff: Mutex::default(),
            play_order: Mutex::default(),
            pcm: broadcast::channel(256).0,
            mp3: broadcast::channel(512).0,
            aac: broadcast::channel(512).0,
            listeners: AtomicUsize::new(0),
            access: watch::channel(0).0,
            closed: watch::channel(false).0,
        })
    }

    pub fn is_closed(&self) -> bool {
        *self.closed.borrow()
    }

    pub fn visibility(&self) -> Visibility {
        self.row.lock().unwrap().visibility
    }
}

/// `a-z0-9-`, 2 to 32 characters (SPEC 10).
pub fn valid_id(id: &str) -> bool {
    (2..=32).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Starts the radio's playout loop and its encoders.
pub fn start(state: &AppState, radio: &Arc<Radio>) {
    if let Err(e) = std::fs::create_dir_all(state.hls_dir(&radio.id)) {
        tracing::warn!(radio = %radio.id, "cannot create the HLS folder: {e}");
    }
    tokio::spawn(playout::run(state.clone(), radio.clone()));
    outputs::spawn_for(state, radio);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radio_ids() {
        for ok in ["main", "lobby", "night-shift", "a1"] {
            assert!(valid_id(ok), "{ok}");
        }
        for bad in ["a", "Main", "night shift", "x/y", "über", &"a".repeat(33)] {
            assert!(!valid_id(bad), "{bad}");
        }
    }

    #[test]
    fn a_new_radio_is_open_and_closes_once() {
        let r = Radio::new(RadioRow { id: "lobby".into(), name: "Lobby".into(), visibility: Visibility::Public, listen_key: "k".into(), created_at: String::new() });
        assert!(!r.is_closed());
        r.closed.send_replace(true);
        assert!(r.is_closed());
    }
}
