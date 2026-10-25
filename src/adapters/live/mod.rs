//! Live streams (arc42 8.4): one station per live playlist, behind the `LiveStreams`, `LiveListening` and `Relay` ports.

pub mod mixer;
pub mod relay;
pub mod station;

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, atomic::Ordering},
};

use crate::{
    domain::{
        Playlist, PlaylistRef,
        live::StreamFormat,
        relay::{Caller, RelayAddresses, RelayStatus},
    },
    usecases::ports::{Listen, LiveListening, LiveSong, LiveStreams, OnAir, Relay},
};
pub use station::ListenerGuard;
use station::{Station, aac_args, encoder, hls_args, mp3_args, playout};

/// A `Tell` that passes `what` to the hook, once one is set.
fn tell(hook: &Hook, what: Changed) -> station::Tell {
    let hook = hook.clone();
    Arc::new(move || {
        if let Some(f) = hook.get() {
            f(what.clone());
        }
    })
}

pub struct StationsConfig {
    pub data_dir: PathBuf,
    pub crossfade_s: f32,
    pub mp3: bool,
    pub aac: bool,
    pub hls: bool,
    /// MediaMTX, when it relays the live playlists over RTSP, SRT and RTMP.
    pub relay: Option<relay::Target>,
}

/// What changed on its own, for the web app (arc42 3.5).
#[derive(Debug, Clone, PartialEq)]
pub enum Changed {
    /// A live playlist's song on air or its listeners.
    Live(String),
    /// MediaMTX's state or what it carries.
    Relay,
}

type Hook = Arc<std::sync::OnceLock<Box<dyn Fn(Changed) + Send + Sync>>>;

pub struct Stations {
    cfg: StationsConfig,
    on: Mutex<HashMap<String, Arc<Station>>>,
    seen: Arc<Mutex<relay::Seen>>,
    hook: Hook,
}

impl Stations {
    /// Call inside the tokio runtime: with a relay, it starts watching MediaMTX.
    pub fn new(cfg: StationsConfig) -> Arc<Self> {
        let (seen, hook): (_, Hook) = (Arc::new(Mutex::new(relay::Seen::default())), Arc::default());
        if let Some(t) = cfg.relay.clone() {
            tokio::spawn(relay::watch(t, seen.clone(), tell(&hook, Changed::Relay)));
        }
        Arc::new(Self { cfg, on: Mutex::new(HashMap::new()), seen, hook })
    }

    /// Sets what to tell when a live playlist or MediaMTX changes on its own; set once, at start.
    pub fn on_change(&self, f: impl Fn(Changed) + Send + Sync + 'static) {
        let _ = self.hook.set(Box::new(f));
    }

    fn get(&self, id: &str) -> Option<Arc<Station>> {
        self.on.lock().unwrap().get(id).cloned()
    }

    fn hls_folder(&self, id: &str) -> PathBuf {
        self.cfg.data_dir.join("live").join(id).join("hls")
    }

    fn start(&self, p: &Playlist, songs: &[LiveSong]) -> Arc<Station> {
        let st = Station::new(p, songs, self.cfg.data_dir.clone(), self.cfg.relay.clone(), tell(&self.hook, Changed::Live(p.id.clone())));
        tokio::spawn(playout(st.clone(), self.cfg.crossfade_s));
        if self.cfg.mp3 {
            tokio::spawn(encoder(st.clone(), "mp3", mp3_args, Some(st.mp3.clone()), false));
        }
        if self.cfg.aac {
            tokio::spawn(encoder(st.clone(), "aac", aac_args, Some(st.aac.clone()), false));
        }
        if self.cfg.hls {
            let dir = self.hls_folder(&p.id);
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

impl LiveListening for Stations {
    fn listen(&self, id: &str, format: StreamFormat) -> Option<Listen> {
        let st = self.get(id)?;
        let bytes = match format {
            StreamFormat::Mp3 if self.cfg.mp3 => st.mp3.subscribe(),
            StreamFormat::Aac if self.cfg.aac => st.aac.subscribe(),
            _ => return None,
        };
        Some(Listen { bytes, moved: st.moved.subscribe(), guard: Box::new(ListenerGuard::new(st.listeners.clone(), st.tell.clone())) })
    }

    fn now_title(&self, id: &str) -> Option<String> {
        self.get(id)?.now.lock().unwrap().as_ref().map(|s| s.title.clone())
    }

    fn hls_dir(&self, id: &str) -> PathBuf {
        self.hls_folder(id)
    }
}

impl Relay for Stations {
    fn caller(&self, user: &str, password: &str) -> Option<Caller> {
        self.cfg.relay.as_ref().map(|t| relay::caller(t, user, password))
    }

    fn status(&self, live: &[(PlaylistRef, String)]) -> Option<RelayStatus> {
        let t = self.cfg.relay.as_ref()?;
        Some(relay::status(&self.seen.lock().unwrap_or_else(|e| e.into_inner()), t, live, chrono::Utc::now()))
    }

    fn addresses(&self, path: &str) -> Option<RelayAddresses> {
        self.cfg.relay.as_ref().map(|t| relay::addresses(t, path))
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
