//! Builds the app from config with the real adapters, and starts the engines' health checks.

use std::sync::Arc;

use crate::adapters::{
    engine::{self, HttpEngine},
    ffmpeg::Ffmpeg,
    live::{Stations, StationsConfig, relay},
    passwords::Argon2Passwords,
    sqlite::{Db, SqliteAccount, SqlitePlaylists, SqliteSongs},
    system::{RandomIds, SystemClock},
    text::{self, HttpTextEngine},
};
use crate::usecases::{
    Account, Live, Making, MakingPorts, Playlists, Songs,
    ports::{Ids, MusicEngine, TextEngine},
};
use anyhow::{Context, Result};

use crate::web::{
    config::Config,
    events::Events,
    metrics::Metrics,
    push::{self, Change},
    state::{App, Shared},
};

/// `admin` is `MAIO_ADMIN_USER` and `MAIO_ADMIN_PASSWORD`, needed only while there is no account.
pub fn start(cfg: &Config, admin: (Option<&str>, Option<&str>)) -> Result<Shared> {
    std::fs::create_dir_all(&cfg.data_dir).with_context(|| format!("creating {}", cfg.data_dir.display()))?;
    let db = Arc::new(Db::open(&cfg.data_dir.join("library.db"))?);
    let (songs, playlists) = (Arc::new(SqliteSongs(db.clone())), Arc::new(SqlitePlaylists(db.clone())));
    let files = Arc::new(Ffmpeg::new(&cfg.data_dir, cfg.store_format(), cfg.storage.bitrate_k)?);
    let meter = opentelemetry::global::meter("maio");
    let metrics = Arc::new(Metrics::new(&meter));
    let (ids, clock, events) = (Arc::new(RandomIds), Arc::new(SystemClock), Arc::new(Events::new(metrics.clone())));

    let account = Account::new(Arc::new(SqliteAccount(db)), Arc::new(Argon2Passwords), ids.clone(), clock.clone(), cfg.api_token.clone());
    account.ensure(admin.0, admin.1)?;

    // What changes on its own is sent by the adapter where it happens (ADR 0018), straight into the queue the
    // request handlers' changes go through; one task tells them in the order they were sent.
    let (queue, changes) = tokio::sync::mpsc::unbounded_channel::<Change>();
    let engines: Vec<Arc<HttpEngine>> = cfg.engine_configs().into_iter().map(|c| Arc::new(HttpEngine::new(c, queue.clone()))).collect();
    for e in &engines {
        tokio::spawn(engine::watch(e.clone()));
    }
    let text_engines: Vec<Arc<HttpTextEngine>> = cfg.text_engine_configs().into_iter().map(|c| Arc::new(HttpTextEngine::new(c))).collect();
    for t in &text_engines {
        tokio::spawn(text::watch(t.clone()));
    }
    let making = Making::new(
        MakingPorts {
            engines: engines.iter().map(|e| e.clone() as Arc<dyn MusicEngine>).collect(),
            text_engines: text_engines.into_iter().map(|t| t as Arc<dyn TextEngine>).collect(),
            songs: songs.clone(),
            files: files.clone(),
            ids: ids.clone(),
            clock: clock.clone(),
            notifier: events.clone(),
        },
        cfg.make_settings(),
    );
    metrics.watch_engines(&meter, making.clone());

    let relay = cfg.relay.enabled.then(|| relay::Target {
        publish: cfg.relay.publish.clone(),
        api: cfg.relay.api.clone(),
        host: cfg.relay.host.clone(),
        rtsp_port: cfg.relay.rtsp_port,
        srt_port: cfg.relay.srt_port,
        rtmp_port: cfg.relay.rtmp_port,
        secret: ids.secret(),
    });
    let stations = Stations::new(
        StationsConfig {
            data_dir: cfg.data_dir.clone(),
            crossfade_s: cfg.crossfade_s,
            mp3: cfg.serve.mp3,
            aac: cfg.serve.aac,
            hls: cfg.serve.hls,
            relay,
        },
        queue,
    );
    let app = Arc::new(App {
        account,
        songs: Songs::new(songs.clone(), playlists.clone(), files),
        live: Live::new(playlists.clone(), stations.clone(), stations.clone()),
        playlists: Playlists::new(playlists, songs, ids.clone(), clock, stations.clone(), cfg.max_live),
        making,
        events,
        moods: cfg.moods.clone(),
        genres: cfg.genres.clone(),
        data_dir: cfg.data_dir.clone(),
        secure_cookies: cfg.secure_cookies,
        public_origin: cfg.public_origin.clone(),
        metrics,
        stopping: tokio::sync::watch::channel(false).0,
        serve: cfg.serve.clone(),
    });
    tokio::spawn(push::run(app.clone(), changes));
    // Playlists that were live before a restart go back on air.
    app.playlists.resume()?;
    Ok(app)
}
