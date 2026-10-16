#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use axum::{
    Json, Router,
    extract::{Path, Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::get,
};
use maio::config::EngineConfig;
use serde_json::{Value, json};

#[derive(Clone, Copy)]
pub enum Outcome {
    Done,
    Fail { retryable: bool },
    Vanish,
    Hang,
}

#[derive(Clone)]
struct Fake {
    id: &'static str,
    commercial: &'static str,
    outcome: Outcome,
    wav: Arc<Vec<u8>>,
    deleted: Arc<Mutex<Vec<String>>>,
    vocals: bool,
    requests: Arc<Mutex<Vec<Value>>>,
}

pub struct FakeEngine {
    pub url: String,
    pub token: String,
    pub deleted: Arc<Mutex<Vec<String>>>,
    /// Every `request` the engine was sent.
    pub requests: Arc<Mutex<Vec<Value>>>,
}

pub const ENGINE_TOKEN: &str = "engine-secret";

async fn auth(req: Request, next: Next) -> Response {
    let ok = req.headers().get("authorization").and_then(|v| v.to_str().ok()) == Some(&format!("Bearer {ENGINE_TOKEN}"));
    if ok { next.run(req).await } else { StatusCode::UNAUTHORIZED.into_response() }
}

/// A minimal engine server speaking SPEC section 4, with a scripted outcome for every job.
pub async fn spawn_fake_engine(id: &'static str, commercial: &'static str, outcome: Outcome) -> FakeEngine {
    spawn_fake_engine_with(id, commercial, outcome, false).await
}

/// As `spawn_fake_engine`; with `vocals` it can sing, and done jobs report what an ACE-Step engine would:
/// lyrics and metas when it wrote the lyrics, the given lyrics otherwise.
pub async fn spawn_fake_engine_with(id: &'static str, commercial: &'static str, outcome: Outcome, vocals: bool) -> FakeEngine {
    let dir = tempfile::tempdir().unwrap();
    let wav_path = dir.path().join("tone.wav");
    maio::media::test_tone(&wav_path, 3).await.unwrap();
    let fake = Fake { id, commercial, outcome, wav: Arc::new(std::fs::read(&wav_path).unwrap()), deleted: Arc::default(), vocals, requests: Arc::default() };
    let (deleted, requests) = (fake.deleted.clone(), fake.requests.clone());

    let app = Router::new()
        .route("/health", get(|| async { Json(json!({"status": "ok"})) }))
        .route("/info", get(|State(f): State<Fake>| async move {
            Json(json!({"protocol": 1, "id": f.id, "name": f.id, "version": "1",
                "licence": {"name": "Test", "commercial": f.commercial, "url": "", "note": "limited test licence"},
                "capabilities": {"min_seconds": 1, "max_seconds": 600, "vocals": f.vocals, "tempo": true, "concurrency": 1},
                "realtime_factor": 0.1}))
        }))
        .route("/jobs", axum::routing::post(|State(f): State<Fake>, Json(b): Json<Value>| async move {
            f.requests.lock().unwrap().push(b["request"].clone());
            (StatusCode::ACCEPTED, Json(json!({"job_id": "j1"})))
        }))
        .route("/jobs/{id}", get(|State(f): State<Fake>, Path(id): Path<String>| async move {
            let body: Value = match f.outcome {
                Outcome::Done => {
                    let mut body = json!({"job_id": id, "state": "done", "seconds": 3.0, "seed": 1});
                    let last = f.requests.lock().unwrap().last().cloned().unwrap_or_default();
                    if last["write_lyrics"] == true {
                        body["lyrics"] = json!("[verse]\nengine verse\n[chorus]\nengine sang this\n");
                        body["metas"] = json!({"bpm": 92, "key": "C major", "time_signature": "4"});
                    } else if last["lyrics"].is_string() {
                        body["lyrics"] = last["lyrics"].clone();
                    }
                    body
                }
                Outcome::Fail { retryable } => json!({"job_id": id, "state": "failed",
                    "error": {"code": "boom", "message": "scripted failure", "retryable": retryable}}),
                Outcome::Vanish => return StatusCode::NOT_FOUND.into_response(),
                Outcome::Hang => json!({"job_id": id, "state": "running", "progress": 0.5}),
            };
            Json(body).into_response()
        }).delete(|State(f): State<Fake>, Path(id): Path<String>| async move {
            f.deleted.lock().unwrap().push(id);
            StatusCode::NO_CONTENT
        }))
        .route("/jobs/{id}/audio", get(|State(f): State<Fake>| async move {
            ([("content-type", "audio/wav")], f.wav.as_ref().clone())
        }))
        .layer(middleware::from_fn(auth))
        .with_state(fake);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    std::mem::forget(dir); // keep the tone file for the life of the test process
    FakeEngine { url, token: ENGINE_TOKEN.into(), deleted, requests }
}

pub fn engine_config(f: &FakeEngine, id: &str) -> EngineConfig {
    EngineConfig { id: id.into(), url: f.url.clone(), token_env: "UNUSED".into(), token: f.token.clone() }
}

use maio::{config, engine::Engine, state::AppState, store::Store};

pub const BASE_TOML: &str = r#"
moods = ["calm", "tense"]
genres = ["lo-fi", "synthwave"]
crossfade_s = 0.5

"#;

/// State with a temp data dir and the given fake engines, already checked and `up`.
pub async fn test_state(engines: Vec<(FakeEngine, &str)>, extra_toml: &str) -> (AppState, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let text = format!("data_dir = {:?}\n{BASE_TOML}\n{extra_toml}", dir.path());
    let cfg = config::parse(&text, |k| Some(format!("tok-{k}"))).unwrap();
    let mut list = Vec::new();
    for (fake, id) in &engines {
        let e = Arc::new(Engine::new(engine_config(fake, id)));
        e.check().await;
        list.push(e);
    }
    let state = AppState::with_engines(cfg, Store::open_in_memory().unwrap(), list).unwrap();
    (state, dir)
}

pub fn sample_track(id: &str, mood: Option<&str>, made_by: Option<&str>, file: &str) -> maio::store::Track {
    maio::store::Track {
        id: id.into(), title: format!("title {id}"), file: file.into(), prompt: "p".into(),
        mood: mood.map(Into::into), genre: None, energy: None, engine: "fake".into(), licence: "CC0-1.0".into(),
        commercial: "yes".into(), seconds: 3.0, bpm: None, vocals: false, seed: 1, favourite: false,
        made_by: made_by.map(Into::into), requested_by: "me".into(), created_at: maio::store::now(), bytes: 1_000,
        title_source: "words".into(), artist: "Maio".into(), album: "Requests".into(), key: None, time_signature: None,
        lyrics: None, vocal_language: None, description: None, lyrics_engine: None, title_engine: None,
    }
}

pub const SITE: &str = "http://localhost:8080";

/// Adds a person whose session id is `sess-{username}` (password "correct horse battery").
pub fn person(state: &AppState, username: &str, role: &str) -> String {
    state.store.insert_user(&maio::store::User {
        username: username.into(), name: username.into(), role: role.into(),
        hash: maio::users::hash_password("correct horse battery"), must_change: false,
        created_at: maio::store::now(), last_seen_at: None,
    }).unwrap();
    let id = format!("sess-{username}");
    state.store.create_session(&id, username, i64::MAX / 2, false).unwrap();
    id
}

/// Stores a playlist (unless it is stored or built in) and puts it on air on `radio`.
pub fn on_air(state: &AppState, radio: &str, p: &maio::playlists::Playlist) {
    if maio::playlists::find(&state.store, &p.id).unwrap().is_none() {
        state.store.insert_playlist(p).unwrap();
    }
    state.store.set_on_air(radio, Some(&p.id)).unwrap();
}

pub fn playlist(id: &str, kind: maio::playlists::Kind, order: maio::playlists::Order) -> maio::playlists::Playlist {
    maio::playlists::Playlist { id: id.into(), name: format!("name {id}"), kind, order, max_tracks: None, recipe: None, rule: None, created_at: maio::store::now() }
}

#[derive(Clone, Copy)]
pub enum FakeReply {
    Text(&'static str),
    Status(u16),
    Hang,
}

pub struct FakeText {
    pub url: String,
    /// Every request: (path, headers as "name: value" lines, body).
    pub seen: Arc<Mutex<Vec<(String, String, Value)>>>,
}

/// A chat API speaking both the Anthropic Messages and the OpenAI chat shapes.
pub async fn spawn_fake_text(reply: FakeReply) -> FakeText {
    let seen: Arc<Mutex<Vec<(String, String, Value)>>> = Arc::default();
    let log = seen.clone();
    let handler = move |uri: axum::http::Uri, headers: axum::http::HeaderMap, Json(body): Json<Value>| {
        let log = log.clone();
        async move {
            let h = headers.iter().map(|(k, v)| format!("{k}: {}", v.to_str().unwrap_or(""))).collect::<Vec<_>>().join("\n");
            log.lock().unwrap().push((uri.path().to_string(), h, body));
            match reply {
                FakeReply::Hang => {
                    tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
                    StatusCode::OK.into_response()
                }
                FakeReply::Status(code) => StatusCode::from_u16(code).unwrap().into_response(),
                FakeReply::Text(t) if uri.path().ends_with("/messages") => Json(json!({"content": [{"type": "text", "text": t}]})).into_response(),
                FakeReply::Text(t) => Json(json!({"choices": [{"message": {"role": "assistant", "content": t}}]})).into_response(),
            }
        }
    };
    let app = Router::new()
        .route("/v1/messages", axum::routing::post(handler.clone()))
        .route("/v1/chat/completions", axum::routing::post(handler));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    FakeText { url, seen }
}

/// Config text for one text engine of `kind` at `url` (anthropic gets api_key_env T_TEXT, so key tok-T_TEXT).
pub fn text_engine_toml(id: &str, kind: &str, url: &str) -> String {
    match kind {
        "anthropic" => format!("[[text_engine]]\nid = \"{id}\"\nkind = \"anthropic\"\nmodel = \"m\"\nurl = \"{url}\"\napi_key_env = \"T_TEXT\"\n"),
        _ => format!("[[text_engine]]\nid = \"{id}\"\nkind = \"openai\"\nmodel = \"m\"\nurl = \"{url}/v1\"\n"),
    }
}
