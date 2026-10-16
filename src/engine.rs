use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

use crate::{config::EngineConfig, error::ApiError, request::Normalized};

pub const PROTOCOL: u32 = 1;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct EngineInfo {
    pub protocol: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub licence: Licence,
    pub capabilities: Caps,
    pub realtime_factor: f32,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Licence {
    pub name: String,
    pub commercial: Commercial,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Commercial {
    Yes,
    Limited,
    No,
}

impl Commercial {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Yes => "yes",
            Self::Limited => "limited",
            Self::No => "no",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Caps {
    pub min_seconds: u32,
    pub max_seconds: u32,
    pub vocals: bool,
    pub tempo: bool,
    pub concurrency: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Health {
    Unknown,
    Up,
    Loading,
    Down,
    Incompatible,
}

#[derive(Debug, Clone, Serialize)]
pub struct EngineRequest {
    pub prompt: String,
    pub seconds: u32,
    pub bpm: Option<u32>,
    pub vocals: bool,
    pub lyrics: Option<String>,
    pub seed: i64,
    /// Sent only for vocals, and `write_lyrics` only when set, so instrumental jobs still suit engine servers
    /// from before these fields (they forbid unknown fields).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub vocal_language: String,
    /// Set by the job when Maio has no lyrics and ACE-Step should write them (SPEC 10, Lyrics).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub write_lyrics: bool,
}

impl From<&Normalized> for EngineRequest {
    fn from(n: &Normalized) -> Self {
        Self { prompt: n.prompt.clone(), seconds: n.seconds, bpm: n.bpm, vocals: n.vocals, lyrics: n.lyrics.clone(), seed: n.seed,
               vocal_language: if n.vocals { n.vocal_language.clone() } else { String::new() }, write_lyrics: false }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteJob {
    pub job_id: String,
    pub state: String,
    #[serde(default)]
    pub progress: f32,
    pub seconds: Option<f64>,
    pub seed: Option<i64>,
    pub error: Option<RemoteError>,
    #[serde(default)]
    pub lyrics: Option<String>,
    #[serde(default)]
    pub metas: Option<Metas>,
}

/// What an engine learned while making a track (SPEC 10, Lyrics): ACE-Step reports these when its planning model runs.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Metas {
    pub bpm: Option<u32>,
    pub key: Option<String>,
    pub time_signature: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteError {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub retryable: bool,
}

#[derive(Debug)]
pub enum CallError {
    Busy,
    NotFound,
    Rejected(String),
    Transport(String),
}

fn transport(e: impl std::fmt::Display) -> CallError {
    CallError::Transport(e.to_string())
}

struct Status {
    info: Option<EngineInfo>,
    health: Health,
    failures: u32,
}

/// One engine server (SPEC 4).
pub struct Engine {
    pub cfg: EngineConfig,
    http: reqwest::Client,
    status: Mutex<Status>,
}

impl Engine {
    pub fn new(cfg: EngineConfig) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(60))
            .build()
            .expect("http client");
        Self { cfg, http, status: Mutex::new(Status { info: None, health: Health::Unknown, failures: 0 }) }
    }

    /// For tests: an engine with a known state and no server behind it.
    pub fn with_state(cfg: EngineConfig, info: Option<EngineInfo>, health: Health) -> Self {
        let e = Self::new(cfg);
        {
            let mut s = e.status.lock().unwrap();
            s.info = info;
            s.health = health;
        }
        e
    }

    pub fn snapshot(&self) -> (Option<EngineInfo>, Health) {
        let s = self.status.lock().unwrap();
        (s.info.clone(), s.health)
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.cfg.url.trim_end_matches('/'), path)
    }

    fn get(&self, path: &str) -> reqwest::RequestBuilder {
        self.http.get(self.url(path)).bearer_auth(&self.cfg.token)
    }

    /// One health check (SPEC 4, Lifecycle): `down` after 3 failures; `/info` refreshed whenever the engine comes up.
    pub async fn check(&self) {
        let code = self.get("/health").timeout(Duration::from_secs(10)).send().await.ok().map(|r| r.status().as_u16());
        match code {
            Some(200) => {
                let needs_info = {
                    let s = self.status.lock().unwrap();
                    s.info.is_none() || s.health != Health::Up
                };
                if !needs_info {
                    self.status.lock().unwrap().failures = 0;
                    return;
                }
                match self.fetch_info().await {
                    Ok(info) => {
                        let mut s = self.status.lock().unwrap();
                        s.failures = 0;
                        s.health = if info.protocol == PROTOCOL { Health::Up } else { Health::Incompatible };
                        s.info = Some(info);
                    }
                    Err(e) => {
                        tracing::warn!(engine = %self.cfg.id, "reading /info: {e}");
                        self.fail();
                    }
                }
            }
            Some(503) => {
                let mut s = self.status.lock().unwrap();
                s.failures = 0;
                s.health = Health::Loading;
            }
            _ => self.fail(),
        }
    }

    /// Counts a failed contact; `down` after 3 in a row. Job calls report transport errors here too.
    pub fn fail(&self) {
        let mut s = self.status.lock().unwrap();
        s.failures += 1;
        if s.failures >= 3 {
            s.health = Health::Down;
        }
    }

    async fn fetch_info(&self) -> Result<EngineInfo, String> {
        let r = self.get("/info").send().await.map_err(|e| e.to_string())?;
        if !r.status().is_success() {
            return Err(format!("status {}", r.status()));
        }
        r.json().await.map_err(|e| e.to_string())
    }

    pub async fn submit(&self, req: &EngineRequest) -> Result<String, CallError> {
        #[derive(Deserialize)]
        struct Created {
            job_id: String,
        }
        let r = self.http.post(self.url("/jobs")).bearer_auth(&self.cfg.token)
            .json(&serde_json::json!({"request": req})).send().await.map_err(transport)?;
        match r.status().as_u16() {
            200 | 202 => r.json::<Created>().await.map(|c| c.job_id).map_err(transport),
            429 => Err(CallError::Busy),
            _ => {
                let status = r.status();
                Err(CallError::Rejected(format!("{status}: {}", r.text().await.unwrap_or_default())))
            }
        }
    }

    pub async fn job(&self, id: &str) -> Result<RemoteJob, CallError> {
        let r = self.get(&format!("/jobs/{id}")).send().await.map_err(transport)?;
        match r.status().as_u16() {
            200 => r.json().await.map_err(transport),
            404 => Err(CallError::NotFound),
            _ => Err(CallError::Transport(format!("status {}", r.status()))),
        }
    }

    pub async fn download(&self, id: &str, dst: &Path) -> Result<(), CallError> {
        let r = self.get(&format!("/jobs/{id}/audio")).timeout(Duration::from_secs(600)).send().await.map_err(transport)?;
        if !r.status().is_success() {
            return Err(CallError::Transport(format!("audio status {}", r.status())));
        }
        let mut file = tokio::fs::File::create(dst).await.map_err(transport)?;
        let mut body = r.bytes_stream();
        while let Some(chunk) = body.next().await {
            file.write_all(&chunk.map_err(transport)?).await.map_err(transport)?;
        }
        file.flush().await.map_err(transport)
    }

    pub async fn delete(&self, id: &str) {
        let _ = self.http.delete(self.url(&format!("/jobs/{id}"))).bearer_auth(&self.cfg.token).send().await;
    }
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine").field("id", &self.cfg.id).field("url", &self.cfg.url).finish_non_exhaustive()
    }
}

pub fn can_serve(info: &EngineInfo, n: &Normalized) -> bool {
    (info.capabilities.min_seconds..=info.capabilities.max_seconds).contains(&n.seconds) && (!n.vocals || info.capabilities.vocals)
}

/// Engine choice (SPEC 3): the first healthy engine in config order that can meet the request.
pub fn choose(engines: &[Arc<Engine>], n: &Normalized, exclude: &[String]) -> Result<Arc<Engine>, ApiError> {
    if let Some(want) = &n.engine {
        if !engines.iter().any(|e| &e.cfg.id == want) {
            return Err(ApiError::invalid("unknown_engine", format!("no engine {want}")));
        }
    }
    let mut blocked_noncommercial = false;
    for e in engines {
        if exclude.contains(&e.cfg.id) || n.engine.as_ref().is_some_and(|w| w != &e.cfg.id) {
            continue;
        }
        let (info, health) = e.snapshot();
        let Some(info) = info else { continue };
        if !can_serve(&info, n) {
            if n.engine.is_some() {
                return Err(ApiError::invalid("engine_cannot_serve", format!("engine {} cannot make this request", e.cfg.id)));
            }
            continue;
        }
        if health != Health::Up {
            continue;
        }
        if info.licence.commercial == Commercial::No && !n.allow_noncommercial {
            blocked_noncommercial = true;
            continue;
        }
        return Ok(e.clone());
    }
    if blocked_noncommercial {
        return Err(ApiError::conflict("noncommercial_engine", "only a non-commercial engine can make this; resend with allow_noncommercial"));
    }
    Err(ApiError::unavailable("engine_unavailable", "no healthy engine can make this request"))
}

pub async fn health_loop(engine: Arc<Engine>) {
    loop {
        engine.check().await;
        tokio::time::sleep(Duration::from_secs(30)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    pub fn info(id: &str, commercial: Commercial, max_seconds: u32, vocals: bool) -> EngineInfo {
        EngineInfo {
            protocol: 1,
            id: id.into(),
            name: id.into(),
            version: "1".into(),
            licence: Licence { name: "X".into(), commercial, url: String::new(), note: String::new() },
            capabilities: Caps { min_seconds: 10, max_seconds, vocals, tempo: true, concurrency: 1 },
            realtime_factor: 0.5,
        }
    }

    fn engine(id: &str, commercial: Commercial, health: Health) -> Arc<Engine> {
        let cfg = EngineConfig { id: id.into(), url: "http://127.0.0.1:9".into(), token_env: "X".into(), token: "t".into() };
        Arc::new(Engine::with_state(cfg, Some(info(id, commercial, 600, false)), health))
    }

    fn req(seconds: u32) -> Normalized {
        Normalized {
            prompt: "calm".into(), seconds, bpm: None, vocals: false, lyrics: None, seed: 1, engine: None,
            allow_noncommercial: false, mood: None, genre: None, energy: None, title: "t".into(), vocal_language: "en".into(), lyrics_by: None, user_prompt: None,
        }
    }

    #[test]
    fn chooses_first_healthy_engine_in_order() {
        let engines = vec![engine("down", Commercial::Yes, Health::Down), engine("a", Commercial::Yes, Health::Up), engine("b", Commercial::Yes, Health::Up)];
        assert_eq!(choose(&engines, &req(180), &[]).unwrap().cfg.id, "a");
        assert_eq!(choose(&engines, &req(180), &["a".into()]).unwrap().cfg.id, "b");
    }

    #[test]
    fn noncommercial_needs_the_flag() {
        let engines = vec![engine("nc", Commercial::No, Health::Up)];
        let err = choose(&engines, &req(180), &[]).unwrap_err();
        assert_eq!((err.status, err.code), (StatusCode::CONFLICT, "noncommercial_engine"));
        assert!(choose(&engines, &Normalized { allow_noncommercial: true, ..req(180) }, &[]).is_ok());
    }

    #[test]
    fn explicit_engine_rules() {
        let engines = vec![engine("a", Commercial::Yes, Health::Up)];
        let too_long = Normalized { engine: Some("a".into()), ..req(601) };
        assert_eq!(choose(&engines, &too_long, &[]).unwrap_err().code, "engine_cannot_serve");
        let unknown = Normalized { engine: Some("zzz".into()), ..req(180) };
        assert_eq!(choose(&engines, &unknown, &[]).unwrap_err().code, "unknown_engine");
    }

    #[test]
    fn nothing_healthy_is_503() {
        let engines = vec![engine("a", Commercial::Yes, Health::Loading)];
        assert_eq!(choose(&engines, &req(180), &[]).unwrap_err().status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(choose(&[], &req(180), &[]).unwrap_err().code, "engine_unavailable");
    }

    #[test]
    fn can_serve_checks_length_and_vocals() {
        let i = info("a", Commercial::Yes, 47, false);
        assert!(can_serve(&i, &req(30)));
        assert!(!can_serve(&i, &req(48)));
        assert!(!can_serve(&i, &Normalized { vocals: true, ..req(30) }));
    }

    #[test]
    fn instrumental_requests_look_like_protocol_1_before_lyrics() {
        let v = serde_json::to_value(EngineRequest::from(&req(180))).unwrap();
        assert!(v.get("write_lyrics").is_none() && v.get("vocal_language").is_none(), "older engine servers forbid unknown fields: {v}");
        let mut sung = EngineRequest::from(&Normalized { vocals: true, ..req(180) });
        sung.write_lyrics = true;
        let v = serde_json::to_value(&sung).unwrap();
        assert_eq!((v["write_lyrics"].as_bool(), v["vocal_language"].as_str()), (Some(true), Some("en")));
    }
}
