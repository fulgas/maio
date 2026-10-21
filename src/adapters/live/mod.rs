//! Live streams (arc42 8.4): one station per live playlist, behind the `LiveStreams` port.

pub mod icy;
pub mod mixer;
pub mod relay;
pub mod station;

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, atomic::Ordering},
};

use bytes::Bytes;
use tokio::sync::{broadcast, watch};

use crate::{
    domain::{Playlist, PlaylistRef, relay::RelayStatus},
    usecases::ports::{LiveSong, LiveStreams, OnAir},
};
pub use station::ListenerGuard;
use station::{Station, aac_args, encoder, hls_args, mp3_args, playout};

pub struct StationsConfig {
    pub data_dir: PathBuf,
    pub crossfade_s: f32,
    pub mp3: bool,
    pub aac: bool,
    pub hls: bool,
    /// MediaMTX, when it relays the live playlists over RTSP, SRT and RTMP.
    pub relay: Option<relay::Target>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Mp3,
    Aac,
}

/// One listener's stream: its bytes, a signal when the address moves (end the stream), and its count.
pub struct Listen {
    pub bytes: broadcast::Receiver<Bytes>,
    pub moved: watch::Receiver<u64>,
    pub guard: ListenerGuard,
}

pub struct Stations {
    cfg: StationsConfig,
    on: Mutex<HashMap<String, Arc<Station>>>,
    seen: Arc<Mutex<relay::Seen>>,
}

impl Stations {
    /// Call inside the tokio runtime: with a relay, it starts watching MediaMTX.
    pub fn new(cfg: StationsConfig) -> Arc<Self> {
        let seen = Arc::new(Mutex::new(relay::Seen::default()));
        if let Some(t) = cfg.relay.clone() {
            tokio::spawn(relay::watch(t, seen.clone()));
        }
        Arc::new(Self { cfg, on: Mutex::new(HashMap::new()), seen })
    }

    /// MediaMTX and the live playlists on it (`live`: each playlist and its relay path); `None` without a relay.
    pub fn relay_status(&self, live: &[(PlaylistRef, String)]) -> Option<RelayStatus> {
        let t = self.cfg.relay.as_ref()?;
        Some(relay::status(&self.seen.lock().unwrap_or_else(|e| e.into_inner()), t, live, chrono::Utc::now()))
    }

    fn get(&self, id: &str) -> Option<Arc<Station>> {
        self.on.lock().unwrap().get(id).cloned()
    }

    pub fn listen(&self, id: &str, f: Format) -> Option<Listen> {
        let st = self.get(id)?;
        let bytes = match f {
            Format::Mp3 if self.cfg.mp3 => st.mp3.subscribe(),
            Format::Aac if self.cfg.aac => st.aac.subscribe(),
            _ => return None,
        };
        Some(Listen { bytes, moved: st.moved.subscribe(), guard: ListenerGuard::new(st.listeners.clone()) })
    }

    pub fn now_title(&self, id: &str) -> Option<String> {
        self.get(id)?.now.lock().unwrap().as_ref().map(|s| s.title.clone())
    }

    pub fn hls_dir(&self, id: &str) -> PathBuf {
        self.cfg.data_dir.join("live").join(id).join("hls")
    }

    fn start(&self, p: &Playlist, songs: &[LiveSong]) -> Arc<Station> {
        let st = Station::new(p, songs, self.cfg.data_dir.clone(), self.cfg.relay.clone());
        tokio::spawn(playout(st.clone(), self.cfg.crossfade_s));
        if self.cfg.mp3 {
            tokio::spawn(encoder(st.clone(), "mp3", mp3_args, Some(st.mp3.clone()), false));
        }
        if self.cfg.aac {
            tokio::spawn(encoder(st.clone(), "aac", aac_args, Some(st.aac.clone()), false));
        }
        if self.cfg.hls {
            let dir = self.hls_dir(&p.id);
            if let Err(e) = std::fs::create_dir_all(&dir) {
                tracing::warn!(playlist = %p.id, "cannot create the HLS folder: {e}");
            }
            tokio::spawn(encoder(st.clone(), "hls", move || hls_args(&dir), None, false));
        }
        if let Some(t) = self.cfg.relay.clone() {
            let station = st.clone();
            // Republished on the new path after a move.
            tokio::spawn(encoder(st.clone(), "relay", move || relay::publish_args(&t, &station.relay_path()), None, true));
        }
        st
    }
}

impl LiveStreams for Stations {
    fn play(&self, p: &Playlist, songs: &[LiveSong]) {
        if let Some(st) = self.get(&p.id) {
            st.update(p, songs);
            return;
        }
        let st = self.start(p, songs);
        self.on.lock().unwrap().insert(p.id.clone(), st);
    }

    fn stop(&self, id: &str) {
        if let Some(st) = self.on.lock().unwrap().remove(id) {
            st.close();
        }
        station::forget(&self.cfg.data_dir, id);
    }

    fn on_air(&self, id: &str) -> Option<OnAir> {
        let st = self.get(id)?;
        let relayed = self.seen.lock().unwrap_or_else(|e| e.into_inner()).readers(&st.relay_path());
        let listeners = st.listeners.load(Ordering::Relaxed) + relayed;
        Some(OnAir { now: st.now.lock().unwrap().clone(), next: st.next.lock().unwrap().clone(), listeners })
    }
}
