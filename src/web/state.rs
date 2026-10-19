//! Everything a request handler can reach.

use std::{path::PathBuf, sync::Arc};

use crate::usecases::{Account, Making, Playlists, Songs};
use tokio::sync::watch;

use crate::web::{events::Events, metrics::Metrics};

pub struct App {
    pub account: Account,
    pub songs: Songs,
    pub playlists: Playlists,
    pub making: Arc<Making>,
    pub events: Arc<Events>,
    pub moods: Vec<String>,
    pub genres: Vec<String>,
    /// `Song::file` is relative to this.
    pub data_dir: PathBuf,
    pub secure_cookies: bool,
    /// See `Config::public_origin`.
    pub public_origin: Option<String>,
    pub metrics: Arc<Metrics>,
    /// Flips to `true` when the server stops, so endless responses (event streams) end and shutdown can finish.
    pub stopping: watch::Sender<bool>,
    /// The live streams; the stream routes read their bytes.
    pub stations: Arc<crate::adapters::live::Stations>,
    pub serve: crate::web::config::Serve,
    /// MediaMTX, when it relays live playlists; its secret answers the auth hook.
    pub relay: Option<crate::adapters::live::relay::Target>,
}

impl App {
    /// Tells endless responses to end.
    pub fn stop(&self) {
        self.stopping.send_replace(true);
    }
}

pub type Shared = Arc<App>;
