use std::{
    collections::HashMap,
    ops::Deref,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};

use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::broadcast;

use crate::{
    auth::Limiter,
    config::{Config, Limits},
    engine::Engine,
    jobs::JobStatus,
    queue::Source,
    radio::Radio,
    store::{Store, Track},
};

#[derive(Clone)]
pub struct AppState(Arc<Inner>);

impl Deref for AppState {
    type Target = Inner;
    fn deref(&self) -> &Inner {
        &self.0
    }
}

pub struct Inner {
    pub cfg: Config,
    pub store: Store,
    pub engines: Vec<Arc<Engine>>,
    pub text_engines: Vec<Arc<crate::text::TextEngine>>,
    pub jobs: Mutex<HashMap<String, JobStatus>>,
    pub limiter: Limiter,
    /// Every radio's runtime; `main` always first.
    pub radios: Mutex<Vec<Arc<Radio>>>,
    pub signin: crate::accounts::SignInLimiter,
    /// People's rate limits, resolved from `[limits]`.
    pub limits: Limits,
    /// Maio's password on the MediaMTX relay; new on every start.
    pub relay_secret: String,
    /// Readers per MediaMTX path, refreshed from its control API.
    pub relay_readers: Mutex<HashMap<String, usize>>,
    pub http: reqwest::Client,
    pub events: broadcast::Sender<Event>,
}

/// Delay before a generative playlist may start new jobs after its jobs failed: 5 s doubling to 5 min, reset by a success.
#[derive(Default)]
pub struct Backoff {
    pub until: Option<Instant>,
    pub delay: std::time::Duration,
}

impl Backoff {
    pub fn active(&self) -> bool {
        self.until.is_some_and(|u| Instant::now() < u)
    }
    pub fn failed(&mut self) {
        self.delay = (self.delay * 2).clamp(std::time::Duration::from_secs(5), std::time::Duration::from_secs(300));
        self.until = Some(Instant::now() + self.delay);
    }
    pub fn succeeded(&mut self) {
        *self = Self::default();
    }
}

#[derive(Default)]
pub struct Playout {
    pub stopped: bool,
    pub skip: bool,
    pub now: Option<NowPlaying>,
}

/// The parts of a track that status and events show. Never the prompt (SPEC 6).
#[derive(Debug, Clone, Serialize)]
pub struct TrackBrief {
    pub id: String,
    pub title: String,
    pub mood: Option<String>,
    pub genre: Option<String>,
    pub bpm: Option<u32>,
    pub seconds: f64,
    pub engine: String,
    pub seed: i64,
    pub artist: String,
    pub album: String,
    pub lyrics: Option<String>,
    pub key: Option<String>,
}

impl From<&Track> for TrackBrief {
    fn from(t: &Track) -> Self {
        Self { id: t.id.clone(), title: t.title.clone(), mood: t.mood.clone(), genre: t.genre.clone(), bpm: t.bpm, seconds: t.seconds, engine: t.engine.clone(), seed: t.seed, artist: t.artist.clone(), album: t.album.clone(), lyrics: t.lyrics.clone(), key: t.key.clone() }
    }
}

#[derive(Debug, Clone)]
pub struct NowPlaying {
    pub entry_id: String,
    pub track: TrackBrief,
    pub source: Source,
    pub started_at: String,
    pub started: Instant,
}

impl NowPlaying {
    pub fn json(&self) -> Value {
        let position = self.started.elapsed().as_secs_f64().min(self.track.seconds);
        json!({"entry_id": self.entry_id, "track": self.track, "source": self.source, "started_at": self.started_at,
               "position_s": (position * 10.0).round() / 10.0, "remaining_s": (self.track.seconds - position).round()})
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct OutputStatus {
    pub name: String,
    pub kind: String,
    pub target: String,
    pub state: &'static str,
}

#[derive(Debug, Clone)]
pub struct Event {
    /// The radio it belongs to; `None` goes to every radio's event stream.
    pub radio: Option<String>,
    pub kind: &'static str,
    pub data: Value,
}

impl AppState {
    pub fn new(cfg: Config, store: Store) -> anyhow::Result<Self> {
        let engines = cfg.engines.iter().cloned().map(|c| Arc::new(Engine::new(c))).collect();
        Self::with_engines(cfg, store, engines)
    }

    pub fn with_engines(cfg: Config, store: Store, engines: Vec<Arc<Engine>>) -> anyhow::Result<Self> {
        for dir in ["tracks", "tmp", "hls"] {
            std::fs::create_dir_all(cfg.data_dir.join(dir))?;
        }
        let limits = crate::config::resolve_limits(&cfg.limits)?;
        let radios = store.radios()?.into_iter().map(Radio::new).collect();
        let text_engines = cfg.text_engines.iter().cloned().map(|c| Arc::new(crate::text::TextEngine::new(c))).collect();
        Ok(Self(Arc::new(Inner {
            cfg,
            store,
            engines,
            text_engines,
            jobs: Mutex::default(),
            limiter: Limiter::default(),
            radios: Mutex::new(radios),
            signin: Default::default(),
            limits,
            relay_secret: crate::users::new_secret(),
            relay_readers: Mutex::default(),
            http: reqwest::Client::builder().timeout(std::time::Duration::from_secs(5)).build()?,
            events: broadcast::channel(256).0,
        })))
    }

    pub fn tracks_dir(&self) -> PathBuf {
        self.cfg.data_dir.join("tracks")
    }
    pub fn tmp_dir(&self) -> PathBuf {
        self.cfg.data_dir.join("tmp")
    }
    pub fn hls_dir(&self, radio: &str) -> PathBuf {
        self.cfg.data_dir.join("hls").join(radio)
    }

    pub fn radio(&self, id: &str) -> Option<Arc<Radio>> {
        self.radios.lock().unwrap().iter().find(|r| r.id == id).cloned()
    }

    /// `main` always exists: the store creates it and it cannot be removed.
    pub fn main(&self) -> Arc<Radio> {
        self.radio("main").expect("the main radio exists")
    }

    pub fn text_engine(&self, id: &str) -> Option<Arc<crate::text::TextEngine>> {
        self.text_engines.iter().find(|t| t.cfg.id == id).cloned()
    }

    pub fn radios(&self) -> Vec<Arc<Radio>> {
        self.radios.lock().unwrap().clone()
    }

    /// Tracks queued or playing on any radio: never deleted, never picked twice at once.
    pub fn busy_track_ids(&self) -> Vec<String> {
        self.radios().iter().flat_map(|r| r.queue.lock().unwrap().track_ids()).collect()
    }

    /// `radio: None` goes to every radio's event stream.
    pub fn emit(&self, radio: Option<&str>, kind: &'static str, data: Value) {
        let _ = self.events.send(Event { radio: radio.map(str::to_string), kind, data });
    }

    /// A radio's upcoming entries, in play order.
    pub fn queue_json(&self, r: &Radio) -> Value {
        let q = r.queue.lock().unwrap();
        Value::Array(q.upcoming().map(|e| serde_json::to_value(e).expect("entry serialises")).collect())
    }

    pub fn emit_queue(&self, r: &Radio) {
        self.emit(Some(&r.id), "queue", self.queue_json(r));
    }
}

