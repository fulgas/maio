//! A whole app on fakes and an in-memory database, and requests as a browser or a script sends them.
#![allow(dead_code)]

use std::{sync::Arc, time::Duration};

use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Request, StatusCode, header, request::Builder},
};
use maio::adapters::sqlite::{Db, SqlitePlaylists, SqliteSongs};
use maio::domain::limits::Rate;
use maio::usecases::{
    Account, MakeSettings, Making, MakingPorts, Playlists, Songs,
    ports::MusicEngine,
    testing::{FakeEngine, FixedClock, MemAccount, MemFiles, PlainPasswords, SeqIds},
};
use maio::web::{App, Shared, events::Events};
use serde_json::{Value, json};
use tower::ServiceExt;

pub const TOKEN: &str = "a-test-api-token-that-is-long-enough";

pub struct World {
    pub app: Shared,
    pub router: Router,
    pub engine: Arc<FakeEngine>,
    pub files: Arc<MemFiles>,
    pub songs: Arc<SqliteSongs>,
    pub playlists: Arc<SqlitePlaylists>,
    pub dir: tempfile::TempDir,
}

impl World {
    /// Puts one song straight into the library; returns its id.
    pub fn songs_with_one(&self) -> String {
        use maio::usecases::ports::SongRepository;
        let song = maio::domain::testing::song("song-1");
        self.songs.insert(&song).unwrap();
        song.id
    }
}

/// A world whose metrics go nowhere.
pub fn world() -> World {
    world_with(&opentelemetry::global::meter("test"))
}

pub fn world_with(meter: &opentelemetry::metrics::Meter) -> World {
    build(meter, None)
}

/// A world served behind a proxy at `https://music.example`.
pub fn world_behind_proxy() -> World {
    build(&opentelemetry::global::meter("test"), Some("https://music.example".into()))
}

fn build(meter: &opentelemetry::metrics::Meter, public_origin: Option<String>) -> World {
    let dir = tempfile::tempdir().unwrap();
    let metrics = Arc::new(maio::web::metrics::Metrics::new(meter));
    let db = Arc::new(Db::open_in_memory().unwrap());
    let (songs, playlists) = (Arc::new(SqliteSongs(db.clone())), Arc::new(SqlitePlaylists(db)));
    let (engine, files, events) = (Arc::new(FakeEngine::up("ace-step")), Arc::new(MemFiles::default()), Arc::new(Events::new(metrics.clone())));
    let (ids, clock) = (Arc::new(SeqIds::default()), Arc::new(FixedClock("2026-10-19T12:00:00Z")));
    let account = Account::new(Arc::new(MemAccount::default()), Arc::new(PlainPasswords::default()), ids.clone(), clock.clone(), Some(TOKEN.into()));
    account.ensure(Some("me"), Some("correct horse")).unwrap();
    let making = Making::new(
        MakingPorts {
            engines: vec![engine.clone() as Arc<dyn MusicEngine>],
            text_engines: vec![],
            songs: songs.clone(),
            files: files.clone(),
            ids: ids.clone(),
            clock: clock.clone(),
            notifier: events.clone(),
        },
        MakeSettings {
            moods: vec!["calm".into()],
            genres: vec!["jazz".into()],
            default_language: "en".into(),
            default_lyrics_by: "ace-step".into(),
            naming_engine: None,
            make_rate: Rate { max: 3, window_s: 600 },
            pending_jobs: 4,
            poll: Duration::from_millis(5),
            lyrics_timeout: Duration::from_millis(100),
            naming_timeout: Duration::from_millis(100),
            stall: Duration::from_secs(10),
        },
    );
    let app = Arc::new(App {
        account,
        songs: Songs::new(songs.clone(), playlists.clone(), files.clone()),
        playlists: Playlists::new(playlists.clone(), songs.clone(), ids, clock),
        making,
        events,
        moods: vec!["calm".into()],
        genres: vec!["jazz".into()],
        data_dir: dir.path().to_path_buf(),
        secure_cookies: false,
        public_origin,
        metrics,
        stopping: tokio::sync::watch::channel(false).0,
    });
    World { router: maio::web::router(app.clone()), app, engine, files, songs, playlists, dir }
}

fn with_body(r: Builder, body: Option<Value>) -> Request<Body> {
    match body {
        Some(v) => r.header(header::CONTENT_TYPE, "application/json").body(Body::from(v.to_string())).unwrap(),
        None => r.body(Body::empty()).unwrap(),
    }
}

/// A request as a browser on this site sends it: `Host`, a matching `Origin`, and the session cookie if any.
pub fn browser(method: &str, path: &str, session: Option<&str>, body: Option<Value>) -> Request<Body> {
    let mut r = Request::builder().method(method).uri(path).header(header::HOST, "localhost:8080").header(header::ORIGIN, "http://localhost:8080");
    if let Some(s) = session {
        r = r.header(header::COOKIE, format!("theme=dark; maio_session={s}"));
    }
    with_body(r, body)
}

/// A request as a script sends it: the API token, no `Origin`.
pub fn script(method: &str, path: &str, body: Option<Value>) -> Request<Body> {
    with_body(Request::builder().method(method).uri(path).header(header::AUTHORIZATION, format!("Bearer {TOKEN}")), body)
}

/// The status, headers and JSON body (`Null` when empty).
pub async fn send(w: &World, req: Request<Body>) -> (StatusCode, HeaderMap, Value) {
    let res = w.router.clone().oneshot(req).await.unwrap();
    let (status, headers) = (res.status(), res.headers().clone());
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let body = if bytes.is_empty() { Value::Null } else { serde_json::from_slice(&bytes).expect("a JSON body") };
    (status, headers, body)
}

/// A page as a browser asks for it: status, headers and the HTML.
pub async fn page(w: &World, path: &str, session: Option<&str>) -> (StatusCode, HeaderMap, String) {
    let res = w.router.clone().oneshot(browser("GET", path, session, None)).await.unwrap();
    let (status, headers) = (res.status(), res.headers().clone());
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    (status, headers, String::from_utf8(bytes.to_vec()).unwrap())
}

/// Signs in as a browser; returns the session id.
pub async fn sign_in(w: &World) -> String {
    let (status, headers, _) = send(w, browser("POST", "/api/v1/session", None, Some(json!({"username": "me", "password": "correct horse"})))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let cookie = headers[header::SET_COOKIE].to_str().unwrap();
    cookie.split(';').next().unwrap().trim_start_matches("maio_session=").to_string()
}
