//! Server-sent events (arc42 3.5, `/events`): `job` carries the job; `song` and `playlist` carry `{"id", "deleted"}`.

use std::sync::Arc;

use crate::domain::{Song, job::Job};
use crate::usecases::ports::Notifier;
use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::broadcast;

use crate::web::metrics::Metrics;

/// A browser that falls this many events behind misses some, and should reload.
const BACKLOG: usize = 256;

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub kind: &'static str,
    pub data: Value,
}

pub struct Events {
    tx: broadcast::Sender<Event>,
    metrics: Arc<Metrics>,
}

impl Events {
    pub fn new(metrics: Arc<Metrics>) -> Self {
        Self { tx: broadcast::channel(BACKLOG).0, metrics }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }

    /// Nobody listening is fine.
    pub fn send(&self, kind: &'static str, data: impl Serialize) {
        match serde_json::to_value(data) {
            Ok(data) => {
                let _ = self.tx.send(Event { kind, data });
            }
            Err(e) => tracing::error!(kind, "an event did not serialise: {e}"),
        }
    }
}

impl Notifier for Events {
    fn job(&self, job: &Job) {
        self.metrics.job_finished(job);
        self.send("job", job);
    }

    /// A new song is in no playlist yet, so it is told as it is (arc42 3.5).
    fn song(&self, song: &Song) {
        let view = crate::usecases::SongView { song: song.clone(), playlists: vec![] };
        self.send("song", json!({"id": song.id, "deleted": false, "song": view}));
    }
}
