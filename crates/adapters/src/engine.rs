//! A music engine server over HTTP (SPEC 6).

use std::{
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

use app::ports::{CallError, EngineRequest, MusicEngine, RemoteJob};
use async_trait::async_trait;
use domain::engine::{EngineInfo, EngineStatus, Health, PROTOCOL};
use futures::StreamExt;
use tokio::io::AsyncWriteExt;

/// `[[engine]]` in config (SPEC 6). Engines take no token.
#[derive(Debug, Clone, PartialEq)]
pub struct EngineConfig {
    pub id: String,
    pub url: String,
}

struct Seen {
    info: Option<EngineInfo>,
    health: Health,
    failures: u32,
}

pub struct HttpEngine {
    cfg: EngineConfig,
    http: reqwest::Client,
    seen: Mutex<Seen>,
}

fn transport(e: impl std::fmt::Display) -> CallError {
    CallError::Transport(e.to_string())
}

impl HttpEngine {
    pub fn new(cfg: EngineConfig) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(60))
            .build()
            .expect("an HTTP client builds");
        Self { cfg, http, seen: Mutex::new(Seen { info: None, health: Health::Unknown, failures: 0 }) }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.cfg.url.trim_end_matches('/'))
    }

    fn seen(&self) -> MutexGuard<'_, Seen> {
        self.seen.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// One health check (SPEC 6, lifecycle): `down` after 3 failures in a row. `/info` is read on every
    /// good check, so a restart into another protocol shows as `incompatible` at once.
    pub async fn check(&self) {
        let status = self.http.get(self.url("/health")).timeout(Duration::from_secs(10)).send().await.ok().map(|r| r.status().as_u16());
        match status {
            Some(200) => match self.fetch_info().await {
                Ok(info) => {
                    let mut s = self.seen();
                    s.failures = 0;
                    s.health = if info.protocol == PROTOCOL { Health::Up } else { Health::Incompatible };
                    s.info = Some(info);
                }
                Err(_) => self.fail(),
            },
            Some(503) => {
                let mut s = self.seen();
                s.failures = 0;
                s.health = Health::Loading;
            }
            _ => self.fail(),
        }
    }

    fn fail(&self) {
        let mut s = self.seen();
        s.failures += 1;
        if s.failures >= 3 {
            s.health = Health::Down;
        }
    }

    async fn fetch_info(&self) -> Result<EngineInfo, reqwest::Error> {
        self.http.get(self.url("/info")).send().await?.error_for_status()?.json().await
    }
}

/// Checks at start and every 30 s, for as long as the server runs (SPEC 6).
pub async fn watch(engine: Arc<HttpEngine>) {
    loop {
        engine.check().await;
        tokio::time::sleep(Duration::from_secs(30)).await;
    }
}

#[async_trait]
impl MusicEngine for HttpEngine {
    fn status(&self) -> EngineStatus {
        let s = self.seen();
        EngineStatus { id: self.cfg.id.clone(), info: s.info.clone(), health: s.health }
    }

    async fn submit(&self, request: &EngineRequest) -> Result<String, CallError> {
        #[derive(serde::Deserialize)]
        struct Created {
            job_id: String,
        }
        let r = self.http.post(self.url("/jobs")).json(&serde_json::json!({ "request": request })).send().await.map_err(transport)?;
        match r.status().as_u16() {
            200 | 202 => r.json::<Created>().await.map(|c| c.job_id).map_err(transport),
            429 => Err(CallError::Busy),
            code => {
                let body: String = r.text().await.unwrap_or_default().chars().take(300).collect();
                Err(CallError::Rejected(format!("{code}: {body}")))
            }
        }
    }

    async fn job(&self, id: &str) -> Result<RemoteJob, CallError> {
        let r = self.http.get(self.url(&format!("/jobs/{id}"))).send().await.map_err(transport)?;
        match r.status().as_u16() {
            200 => r.json().await.map_err(transport),
            404 => Err(CallError::NotFound),
            code => Err(CallError::Transport(format!("job status {code}"))),
        }
    }

    async fn download(&self, id: &str, to: &Path) -> Result<(), CallError> {
        let r = self.http.get(self.url(&format!("/jobs/{id}/audio"))).timeout(Duration::from_secs(600)).send().await.map_err(transport)?;
        if !r.status().is_success() {
            return Err(CallError::Transport(format!("audio status {}", r.status())));
        }
        let mut file = tokio::fs::File::create(to).await.map_err(transport)?;
        let mut body = r.bytes_stream();
        while let Some(chunk) = body.next().await {
            file.write_all(&chunk.map_err(transport)?).await.map_err(transport)?;
        }
        file.flush().await.map_err(transport)
    }

    async fn delete(&self, id: &str) {
        let _ = self.http.delete(self.url(&format!("/jobs/{id}"))).send().await;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::{
        Json, Router,
        extract::{Path as UrlPath, State},
        http::StatusCode,
        routing::{get, post},
    };
    use serde_json::{Value, json};

    use super::*;

    /// What the fake engine answers; tests change it between calls.
    #[derive(Clone)]
    struct Fake {
        health: Arc<Mutex<u16>>,
        protocol: Arc<Mutex<u32>>,
        submit: Arc<Mutex<u16>>,
        posted: Arc<Mutex<Vec<Value>>>,
        deleted: Arc<Mutex<Vec<String>>>,
    }

    fn fake() -> Fake {
        Fake {
            health: Arc::new(Mutex::new(200)),
            protocol: Arc::new(Mutex::new(1)),
            submit: Arc::new(Mutex::new(202)),
            posted: Arc::default(),
            deleted: Arc::default(),
        }
    }

    async fn serve(fake: Fake) -> String {
        let app = Router::new()
            .route("/health", get(|State(f): State<Fake>| async move { StatusCode::from_u16(*f.health.lock().unwrap()).unwrap() }))
            .route(
                "/info",
                get(|State(f): State<Fake>| async move {
                    Json(json!({"protocol": *f.protocol.lock().unwrap(), "id": "ace-step", "name": "ACE-Step", "version": "1.5",
                        "licence": {"name": "MIT", "commercial": "yes", "url": "https://x", "note": ""},
                        "capabilities": {"min_seconds": 10, "max_seconds": 600, "vocals": true, "tempo": true, "concurrency": 1},
                        "realtime_factor": 0.43}))
                }),
            )
            .route(
                "/jobs",
                post(|State(f): State<Fake>, Json(body): Json<Value>| async move {
                    f.posted.lock().unwrap().push(body);
                    (StatusCode::from_u16(*f.submit.lock().unwrap()).unwrap(), Json(json!({"job_id": "j1"})))
                }),
            )
            .route(
                "/jobs/{id}",
                get(|UrlPath(id): UrlPath<String>| async move {
                    if id == "gone" {
                        return (StatusCode::NOT_FOUND, Json(json!({})));
                    }
                    (StatusCode::OK, Json(json!({"job_id": id, "state": "done", "seconds": 179.8, "seed": 7, "lyrics": "[chorus]\nla",
                        "metas": {"bpm": 92, "key": "C major", "time_signature": "4"}})))
                })
                .delete(|State(f): State<Fake>, UrlPath(id): UrlPath<String>| async move {
                    f.deleted.lock().unwrap().push(id);
                    StatusCode::NO_CONTENT
                }),
            )
            .route("/jobs/{id}/audio", get(|| async { ([("content-type", "audio/wav")], b"RIFF-fake-wav".to_vec()) }))
            .with_state(fake);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        url
    }

    fn engine(url: String) -> HttpEngine {
        HttpEngine::new(EngineConfig { id: "ace-step".into(), url })
    }

    fn request() -> EngineRequest {
        EngineRequest { prompt: "calm".into(), seconds: 30, bpm: Some(70), vocals: false, lyrics: None, seed: 7, vocal_language: "en".into(), write_lyrics: false }
    }

    #[tokio::test]
    async fn health_checks_read_info_and_follow_loading_down_and_incompatible() {
        let f = fake();
        let e = engine(serve(f.clone()).await);
        assert_eq!(e.status().health, Health::Unknown);
        e.check().await;
        let s = e.status();
        assert_eq!((s.id.as_str(), s.health, s.info.map(|i| i.realtime_factor)), ("ace-step", Health::Up, Some(0.43)));
        *f.health.lock().unwrap() = 503;
        e.check().await;
        assert_eq!(e.status().health, Health::Loading);
        *f.health.lock().unwrap() = 500;
        e.check().await;
        e.check().await;
        assert_eq!(e.status().health, Health::Loading, "two failures are not down yet");
        e.check().await;
        assert_eq!(e.status().health, Health::Down);
        *f.health.lock().unwrap() = 200;
        *f.protocol.lock().unwrap() = 2;
        e.check().await;
        assert_eq!(e.status().health, Health::Incompatible, "coming back reads /info again");
    }

    #[tokio::test]
    async fn an_unreachable_engine_is_down_after_three_checks() {
        let e = engine("http://127.0.0.1:9".into());
        for _ in 0..3 {
            e.check().await;
        }
        assert_eq!(e.status().health, Health::Down);
    }

    #[tokio::test]
    async fn jobs_are_submitted_polled_downloaded_and_freed() {
        let f = fake();
        let e = engine(serve(f.clone()).await);
        assert_eq!(e.submit(&request()).await.unwrap(), "j1");
        assert_eq!(f.posted.lock().unwrap()[0]["request"]["prompt"], "calm");
        let job = e.job("j1").await.unwrap();
        let metas = job.metas.clone().unwrap();
        assert_eq!((job.state.as_str(), job.seed, metas.key.as_deref(), metas.bpm), ("done", Some(7), Some("C major"), Some(92)));
        assert_eq!(e.job("gone").await.unwrap_err(), CallError::NotFound);
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("j1.wav");
        e.download("j1", &wav).await.unwrap();
        assert_eq!(std::fs::read(&wav).unwrap(), b"RIFF-fake-wav");
        e.delete("j1").await;
        assert_eq!(*f.deleted.lock().unwrap(), ["j1"]);
    }

    #[tokio::test]
    async fn a_busy_engine_a_refusal_and_no_answer_are_told_apart() {
        let f = fake();
        let e = engine(serve(f.clone()).await);
        *f.submit.lock().unwrap() = 429;
        assert_eq!(e.submit(&request()).await.unwrap_err(), CallError::Busy);
        *f.submit.lock().unwrap() = 422;
        assert!(matches!(e.submit(&request()).await.unwrap_err(), CallError::Rejected(m) if m.starts_with("422")));
        assert!(matches!(engine("http://127.0.0.1:9".into()).submit(&request()).await.unwrap_err(), CallError::Transport(_)));
    }

    #[tokio::test]
    async fn a_protocol_change_is_seen_while_the_engine_stays_up() {
        let f = fake();
        let e = engine(serve(f.clone()).await);
        e.check().await;
        assert_eq!(e.status().health, Health::Up);
        *f.protocol.lock().unwrap() = 2;
        e.check().await;
        assert_eq!(e.status().health, Health::Incompatible);
    }
}
