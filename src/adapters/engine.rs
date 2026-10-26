//! ACE-Step 1.5's own HTTP API, `acestep-api` (arc42 3.3).

use std::{
    collections::HashMap,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use crate::domain::engine::{Caps, EngineInfo, EngineStatus, Health};
use crate::domain::{Commercial, Licence};
use crate::usecases::ports::{CallError, Change, Changes, EngineRequest, Metas, MusicEngine, RemoteError, RemoteJob};
use async_trait::async_trait;
use futures::StreamExt;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{io::AsyncWriteExt, sync::watch};

/// `[[engine]]` in config (arc42 3.3).
#[derive(Clone, PartialEq)]
pub struct EngineConfig {
    pub id: String,
    pub url: String,
    /// ACE-Step's API key, when it runs with `ACESTEP_API_KEY`; sent as a bearer token on every call.
    pub api_key: Option<String>,
}

/// By hand, so a key never reaches a log line or a panic message.
impl std::fmt::Debug for EngineConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EngineConfig").field("id", &self.id).field("url", &self.url).field("api_key", &self.api_key.as_ref().map(|_| "***")).finish()
    }
}

/// Generation time over audio length until a song has been measured; ACE-Step turbo on Apple silicon.
const FIRST_RTF: f32 = 0.5;

struct Seen {
    info: Option<EngineInfo>,
    health: Health,
    failures: u32,
    /// Measured generation time over audio length, smoothed.
    rtf: f32,
    /// When each task was released and how long its song is: the measured speed.
    started: HashMap<String, (Instant, u32)>,
    /// A model switch is under way: ACE-Step keeps reporting the old model as up until it ends.
    switching: bool,
    /// The lyrics plan of a song ACE-Step was too busy to take, for when the same song is tried again.
    kept_plan: Option<(EngineRequest, Plan)>,
}

pub struct HttpEngine {
    cfg: EngineConfig,
    http: reqwest::Client,
    /// What Maio knows of it: the latest value, read by anyone, changed only through `update`.
    seen: watch::Sender<Seen>,
    /// Where a change of its status goes, so the web app shows it at once (arc42 3.5).
    changes: Changes,
}

fn transport(e: impl std::fmt::Display) -> CallError {
    CallError::Transport(e.to_string())
}

/// An HTTP failure, said by its cause rather than reqwest's "error sending request for url" (arc42 8.2).
fn http(e: reqwest::Error) -> CallError {
    if e.is_connect() {
        let address = e.url().map(|u| format!("{}:{}", u.host_str().unwrap_or_default(), u.port_or_known_default().unwrap_or_default()));
        return CallError::Unreachable(address.unwrap_or_default());
    }
    CallError::Transport(if e.is_timeout() {
        "no answer in time".into()
    } else if e.is_decode() {
        "an answer Maio does not understand".into()
    } else if let Some(status) = e.status() {
        format!("answered {status}")
    } else {
        let mut cause: &dyn std::error::Error = &e;
        while let Some(next) = cause.source() {
            cause = next;
        }
        cause.to_string()
    })
}

/// What ACE-Step's planning model writes for a song with vocals (`/v1/create_sample`).
#[derive(Deserialize, Clone)]
struct Plan {
    #[serde(default)]
    lyrics: String,
    bpm: Option<u32>,
    #[serde(default)]
    keyscale: String,
    #[serde(default)]
    timesignature: String,
}

/// Every `acestep-api` answer is `{"data": …, "code": 200, "error": null}`.
#[derive(Deserialize)]
struct Wrapped<T> {
    data: Option<T>,
    #[serde(default)]
    error: Option<String>,
}

impl HttpEngine {
    pub fn new(cfg: EngineConfig, changes: Changes) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(60))
            .build()
            .expect("an HTTP client builds");
        let seen =
            Seen { info: None, health: Health::Unknown, failures: 0, rtf: FIRST_RTF, started: HashMap::new(), switching: false, kept_plan: None };
        Self { cfg, http, seen: watch::Sender::new(seen), changes }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.cfg.url.trim_end_matches('/'))
    }

    /// Every call to ACE-Step: the current span's W3C trace context, so ACE-Step's spans (when it runs under
    /// OpenTelemetry) join this trace, and the API key when there is one.
    fn traced(&self, rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        use opentelemetry::propagation::Injector;
        use tracing_opentelemetry::OpenTelemetrySpanExt;
        struct Headers(Vec<(String, String)>);
        impl Injector for Headers {
            fn set(&mut self, key: &str, value: String) {
                self.0.push((key.to_string(), value));
            }
        }
        let mut h = Headers(Vec::new());
        let cx = tracing::Span::current().context();
        opentelemetry::global::get_text_map_propagator(|p| p.inject_context(&cx, &mut h));
        let rb = h.0.into_iter().fold(rb, |rb, (k, v)| rb.header(k, v));
        match &self.cfg.api_key {
            Some(key) => rb.bearer_auth(key),
            None => rb,
        }
    }

    fn seen(&self) -> watch::Ref<'_, Seen> {
        self.seen.borrow()
    }

    /// Changes what Maio knows of the engine. A change of its status (health, model, queue) is sent to the web app at
    /// once, and a change of health is logged.
    fn update(&self, f: impl FnOnce(&mut Seen)) {
        let mut health = None;
        self.seen.send_if_modified(|s| {
            let before = (s.health, s.info.clone());
            f(s);
            let changed = (s.health, &s.info) != (before.0, &before.1);
            health = changed.then_some((before.0, s.health));
            changed
        });
        let Some((before, after)) = health else { return };
        drop(self.changes.send(Change::Engine(self.cfg.id.clone()))); // closed only when the server stops
        if after == before {
            return;
        }
        let id = &self.cfg.id;
        match after {
            Health::Up => tracing::info!(engine = %id, "engine is up"),
            Health::Loading => tracing::info!(engine = %id, "engine is loading its model"),
            Health::Down => tracing::warn!(engine = %id, "engine is down"),
            Health::Incompatible => tracing::warn!(engine = %id, "engine answers, but not as acestep-api; it gets no jobs"),
            Health::Unknown => {}
        }
    }

    async fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, CallError> {
        let r = self.traced(self.http.get(self.url(path))).timeout(Duration::from_secs(10)).send().await.map_err(http)?;
        let w: Wrapped<T> = r.error_for_status().map_err(http)?.json().await.map_err(http)?;
        w.data.ok_or_else(|| CallError::Transport(w.error.unwrap_or_else(|| format!("{path}: no data"))))
    }

    async fn post<T: serde::de::DeserializeOwned>(&self, path: &str, body: Value, timeout: Duration) -> Result<T, CallError> {
        let r = self.traced(self.http.post(self.url(path))).json(&body).timeout(timeout).send().await.map_err(|e| self.call_failed(http(e)))?;
        let status = r.status();
        // ACE-Step's queue is full: try again later, not a refusal (arc42 8.8).
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(CallError::Busy);
        }
        let w: Wrapped<T> = r.json().await.map_err(http)?;
        match w.data {
            Some(d) if status.is_success() => Ok(d),
            _ => Err(CallError::Rejected(format!("{status}: {}", w.error.unwrap_or_default()))),
        }
    }

    /// One health check (arc42 3.3): `down` after 3 failures in a row; loading until ACE-Step's models are up.
    pub async fn check(&self) {
        #[derive(Deserialize)]
        struct H {
            service: String,
            version: String,
            models_initialized: bool,
            loaded_model: Option<String>,
            loaded_lm_model: Option<String>,
        }
        #[derive(Deserialize, Default)]
        struct Jobs {
            #[serde(default)]
            queued: u32,
            #[serde(default)]
            running: u32,
        }
        #[derive(Deserialize)]
        struct Stats {
            #[serde(default)]
            jobs: Jobs,
        }
        #[derive(Deserialize)]
        struct Model {
            name: String,
        }
        #[derive(Deserialize)]
        struct Inventory {
            models: Vec<Model>,
        }
        let h: H = match self.get("/health").await {
            Ok(h) => h,
            // Nothing listens: certain, so down at once. A timeout may be a busy engine: 3 in a row.
            Err(CallError::Unreachable(_)) => return self.mark_down(),
            Err(_) => return self.fail(),
        };
        if h.service != "ACE-Step API" {
            return self.update(|s| (s.failures, s.health) = (0, Health::Incompatible));
        }
        let stats: Option<Stats> = self.get("/v1/stats").await.ok();
        let inventory: Option<Inventory> = self.get("/v1/model_inventory").await.ok();
        let jobs = stats.map(|x| x.jobs).unwrap_or_default();
        self.update(|s| {
            s.failures = 0;
            s.health = if h.models_initialized && !s.switching { Health::Up } else { Health::Loading };
            s.info = Some(EngineInfo {
                id: "ace-step".into(),
                name: "ACE-Step 1.5".into(),
                version: h.version,
                licence: Licence { name: "MIT".into(), commercial: Commercial::Yes, note: String::new() },
                capabilities: Caps { min_seconds: 10, max_seconds: 600, vocals: true, tempo: true, concurrency: 1 },
                realtime_factor: s.rtf,
                model: h.loaded_model.unwrap_or_default(),
                lm_model: h.loaded_lm_model,
                models: inventory.map(|i| i.models.into_iter().map(|m| m.name).collect()).unwrap_or_default(),
                queued: jobs.queued,
                running: jobs.running,
            });
        });
    }

    /// A job's call that cannot connect marks the engine down at once, so new songs are refused before a job
    /// starts; the next health check that gets an answer brings it back (arc42 3.3).
    fn call_failed(&self, e: CallError) -> CallError {
        if matches!(e, CallError::Unreachable(_)) {
            self.mark_down();
        }
        e
    }

    fn mark_down(&self) {
        self.update(|s| (s.failures, s.health) = (3, Health::Down));
    }

    fn fail(&self) {
        self.update(|s| {
            s.failures += 1;
            if s.failures >= 3 {
                s.health = Health::Down;
            }
        });
    }

    /// The first result of a task, or `NotFound`.
    async fn result(&self, id: &str) -> Result<(i64, Value), CallError> {
        #[derive(Deserialize)]
        struct Task {
            status: i64,
            #[serde(default)]
            result: Option<String>,
        }
        let tasks: Vec<Task> = self.post("/query_result", json!({ "task_id_list": [id] }), Duration::from_secs(30)).await?;
        let task = tasks.into_iter().next().ok_or(CallError::NotFound)?;
        // ACE-Step answers an id it does not know (lost in a restart) with an empty result: every real task has one item.
        let first =
            task.result.and_then(|r| serde_json::from_str::<Vec<Value>>(&r).ok()).and_then(|v| v.into_iter().next()).ok_or(CallError::NotFound)?;
        Ok((task.status, first))
    }
}

/// Checks at start and every 10 s, every 5 s after a failed check or while down, for as long as the server runs (arc42 8.8).
pub async fn watch(engine: Arc<HttpEngine>) {
    loop {
        engine.check().await;
        let (health, failures) = {
            let s = engine.seen();
            (s.health, s.failures)
        };
        tokio::time::sleep(next_check(health, failures)).await;
    }
}

/// How long until the next health check: sooner after a failure, so a stopped engine shows as down within seconds,
/// and while down, so songs are taken again soon after it answers.
fn next_check(health: Health, failures: u32) -> Duration {
    Duration::from_secs(if health == Health::Down || failures > 0 { 5 } else { 10 })
}

#[async_trait]
impl MusicEngine for HttpEngine {
    fn status(&self) -> EngineStatus {
        let s = self.seen();
        EngineStatus { id: self.cfg.id.clone(), info: s.info.clone(), health: s.health }
    }

    async fn submit(&self, request: &EngineRequest) -> Result<String, CallError> {
        #[derive(Deserialize)]
        struct Released {
            task_id: String,
        }
        let (mut lyrics, mut bpm, mut key, mut time) = (request.lyrics.clone().unwrap_or_default(), request.bpm, String::new(), String::new());
        let mut plan = None;
        if request.vocals && request.write_lyrics {
            // The plan of this same request, kept when ACE-Step was busy: written once, not again on each try.
            let mut kept = None;
            self.update(|s| kept = s.kept_plan.take());
            let kept = kept.filter(|(r, _)| r == request).map(|(_, p)| p);
            let p: Plan = match kept {
                Some(p) => p,
                None => {
                    // ACE-Step's planning model writes them; loading it the first time takes a while.
                    let body = json!({ "query": request.prompt, "instrumental": false, "vocal_language": request.vocal_language });
                    self.post("/v1/create_sample", body, Duration::from_secs(300)).await?
                }
            };
            (lyrics, key, time) = (p.lyrics.clone(), p.keyscale.clone(), p.timesignature.clone());
            bpm = bpm.or(p.bpm);
            plan = Some(p);
        }
        if !request.vocals {
            lyrics = "[Instrumental]".into();
        }
        let body = json!({
            "prompt": request.prompt, "lyrics": lyrics, "audio_duration": request.seconds, "bpm": bpm,
            "key_scale": key, "time_signature": time, "seed": request.seed, "use_random_seed": false,
            "batch_size": 1, "audio_format": "wav", "thinking": false,
            "vocal_language": if request.vocals { request.vocal_language.as_str() } else { "unknown" },
        });
        let released: Released = match self.post("/release_task", body, Duration::from_secs(60)).await {
            Err(CallError::Busy) => {
                self.update(|s| s.kept_plan = plan.map(|p| (request.clone(), p)));
                return Err(CallError::Busy);
            }
            r => r?,
        };
        self.update(|s| _ = s.started.insert(released.task_id.clone(), (Instant::now(), request.seconds)));
        Ok(released.task_id)
    }

    async fn job(&self, id: &str) -> Result<RemoteJob, CallError> {
        let (status, first) = self.result(id).await?;
        let base = RemoteJob { job_id: id.into(), ..Default::default() };
        match status {
            0 => {
                // ACE-Step's own progress and stage; waiting in its queue is not being made.
                let state = if first["stage"].as_str() == Some("queued") { "queued" } else { "running" };
                let progress = first["progress"].as_f64().map_or(0.0, |p| p.clamp(0.0, 1.0) as f32);
                Ok(RemoteJob { state: state.into(), progress, ..base })
            }
            1 => {
                let metas = &first["metas"];
                let text = |k: &str| metas[k].as_str().filter(|v| !v.is_empty()).map(str::to_string);
                let seconds = metas["duration"].as_f64();
                self.update(|s| {
                    if let (Some((at, _)), Some(secs)) = (s.started.remove(id), seconds) {
                        s.rtf = 0.7 * s.rtf + 0.3 * (at.elapsed().as_secs_f32() / secs.max(1.0) as f32);
                    }
                });
                Ok(RemoteJob {
                    state: "done".into(),
                    progress: 1.0,
                    seconds,
                    seed: first["seed_value"].as_str().and_then(|s| s.split(',').next()).and_then(|s| s.trim().parse().ok()),
                    lyrics: first["lyrics"].as_str().filter(|l| !l.trim().is_empty() && *l != "[Instrumental]").map(str::to_string),
                    metas: Some(Metas { bpm: metas["bpm"].as_u64().map(|b| b as u32), key: text("keyscale"), time_signature: text("timesignature") }),
                    ..base
                })
            }
            _ => {
                self.update(|s| _ = s.started.remove(id));
                let message = first["error"].as_str().unwrap_or("ACE-Step could not make the song").to_string();
                Ok(RemoteJob {
                    state: "failed".into(),
                    error: Some(RemoteError { code: "generation_failed".into(), message, retryable: false }),
                    ..base
                })
            }
        }
    }

    async fn download(&self, id: &str, to: &Path) -> Result<(), CallError> {
        let (_, first) = self.result(id).await?;
        let file = first["file"].as_str().ok_or_else(|| CallError::Transport("the finished task names no file".into()))?;
        let r = self.traced(self.http.get(self.url(file))).timeout(Duration::from_secs(600)).send().await.map_err(|e| self.call_failed(http(e)))?;
        if !r.status().is_success() {
            return Err(CallError::Transport(format!("audio status {}", r.status())));
        }
        let mut out = tokio::fs::File::create(to).await.map_err(transport)?;
        let mut body = r.bytes_stream();
        while let Some(chunk) = body.next().await {
            out.write_all(&chunk.map_err(http)?).await.map_err(transport)?;
        }
        out.flush().await.map_err(transport)
    }

    /// ACE-Step can neither cancel nor delete a task: a cancelled song finishes there and is not fetched,
    /// and ACE-Step's own cleanup removes its file.
    async fn delete(&self, id: &str) {
        self.update(|s| _ = s.started.remove(id));
    }

    async fn switch_model(&self, model: &str) -> Result<(), CallError> {
        self.update(|s| (s.switching, s.health) = (true, Health::Loading));
        let done: Result<Value, CallError> = self.post("/v1/init", json!({ "model": model, "slot": 1 }), Duration::from_secs(600)).await;
        self.update(|s| s.switching = false);
        self.check().await; // what ACE-Step says now: the new model, or the old one after a failure
        done.map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use parking_lot::Mutex;

    use axum::{
        Json, Router,
        extract::State,
        routing::{get, post},
    };
    use serde_json::{Value, json};

    use super::*;

    /// A fake `acestep-api`: what the tests set, and what Maio sent.
    #[derive(Clone, Default)]
    struct Fake {
        initialized: Arc<Mutex<bool>>,
        status: Arc<Mutex<i64>>, // 0 working, 1 done, 2 failed, -1 unknown task
        stage: Arc<Mutex<&'static str>>,
        init_ms: Arc<Mutex<u64>>,
        posted: Arc<Mutex<Vec<(String, Value)>>>,
        /// The Authorization header of every call, health checks included.
        auth: Arc<Mutex<Vec<String>>>,
        /// Answers this many song submissions as ACE-Step does when its queue is full.
        busy: Arc<Mutex<u32>>,
        /// The model ACE-Step reports as loaded; the turbo model when unset.
        model: Arc<Mutex<Option<&'static str>>>,
    }

    fn wrap(data: Value) -> Json<Value> {
        Json(json!({"data": data, "code": 200, "error": null, "timestamp": 0, "extra": null}))
    }

    async fn serve(fake: Fake) -> String {
        let record = |path: &'static str| {
            move |State(f): State<Fake>, headers: axum::http::HeaderMap, Json(body): Json<Value>| async move {
                use axum::response::IntoResponse;
                f.auth.lock().push(headers.get("authorization").and_then(|v| v.to_str().ok()).unwrap_or_default().to_string());
                f.posted.lock().push((path.to_string(), body));
                if path == "/release_task" {
                    let mut busy = f.busy.lock();
                    if *busy > 0 {
                        *busy -= 1;
                        // release_task_route.py: HTTPException(status_code=429, detail="Server busy: queue is full")
                        return (axum::http::StatusCode::TOO_MANY_REQUESTS, Json(json!({"detail": "Server busy: queue is full"}))).into_response();
                    }
                }
                let answer = match path {
                    "/release_task" => wrap(json!({"task_id": "t1", "status": "queued", "queue_position": 1})),
                    "/v1/create_sample" => wrap(json!({"lyrics": "[verse]\nwritten", "bpm": 92, "keyscale": "A minor", "timesignature": "4"})),
                    "/v1/init" => {
                        let ms = *f.init_ms.lock();
                        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
                        wrap(json!({"loaded_model": "acestep-v15-base"}))
                    }
                    _ => {
                        let status = *f.status.lock();
                        // ACE-Step's own shapes (query_result_service.py): an unknown id is "[]" with status 0;
                        // a task being made carries its progress and stage.
                        if status < 0 {
                            return wrap(json!([{"task_id": "t1", "result": "[]", "status": 0}])).into_response();
                        }
                        if status == 0 {
                            let stage = *f.stage.lock();
                            let result = json!([{"file": "", "status": 0, "progress": 0.4, "stage": stage}]);
                            return wrap(json!([{"task_id": "t1", "status": 0, "result": result.to_string()}])).into_response();
                        }
                        let result = json!([{"file": "/v1/audio?path=%2Ftmp%2Ft1.wav", "lyrics": "[verse]\nsung",
                            "metas": {"bpm": 92, "keyscale": "A minor", "timesignature": "4", "duration": 30}, "seed_value": "7"}]);
                        wrap(json!([{"task_id": "t1", "status": status, "result": result.to_string()}]))
                    }
                };
                answer.into_response()
            }
        };
        let app = Router::new()
            .route(
                "/health",
                get(|State(f): State<Fake>, headers: axum::http::HeaderMap| async move {
                    f.auth.lock().push(headers.get("authorization").and_then(|v| v.to_str().ok()).unwrap_or_default().to_string());
                    wrap(json!({"status": "ok", "service": "ACE-Step API", "version": "1.0",
                    "models_initialized": *f.initialized.lock(), "llm_initialized": false,
                    "loaded_model": f.model.lock().unwrap_or("acestep-v15-turbo"), "loaded_lm_model": null}))
                }),
            )
            .route("/v1/stats", get(|| async { wrap(json!({"jobs": {"queued": 2, "running": 1}, "avg_job_seconds": 20.0})) }))
            .route(
                "/v1/model_inventory",
                get(|| async {
                    wrap(json!({"models": [{"name": "acestep-v15-base", "is_loaded": false}, {"name": "acestep-v15-turbo", "is_loaded": true}]}))
                }),
            )
            .route("/v1/audio", get(|| async { b"RIFFfake".to_vec() }))
            .route("/release_task", post(record("/release_task")))
            .route("/query_result", post(record("/query_result")))
            .route("/v1/create_sample", post(record("/v1/create_sample")))
            .route("/v1/init", post(record("/v1/init")))
            .with_state(fake);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        url
    }

    fn engine(url: String) -> HttpEngine {
        telling(url).0
    }

    /// An engine, and what it tells the event queue.
    fn telling(url: String) -> (HttpEngine, tokio::sync::mpsc::UnboundedReceiver<Change>) {
        let (changes, told) = tokio::sync::mpsc::unbounded_channel();
        (HttpEngine::new(EngineConfig { id: "ace-step".into(), url, api_key: None }, changes), told)
    }

    /// How many changes it told since the last look; each names this engine.
    fn told(rx: &mut tokio::sync::mpsc::UnboundedReceiver<Change>) -> usize {
        std::iter::from_fn(|| rx.try_recv().ok()).inspect(|c| assert_eq!(*c, Change::Engine("ace-step".into()))).count()
    }

    fn request() -> EngineRequest {
        EngineRequest {
            prompt: "calm".into(),
            seconds: 30,
            bpm: Some(90),
            vocals: false,
            lyrics: None,
            seed: 7,
            vocal_language: "en".into(),
            write_lyrics: false,
        }
    }

    #[tokio::test]
    async fn health_reads_the_model_the_queue_and_the_installed_models() {
        let fake = Fake::default();
        let e = engine(serve(fake.clone()).await);
        e.check().await;
        assert_eq!(e.status().health, Health::Loading, "models not initialized yet");
        *fake.initialized.lock() = true;
        e.check().await;
        let s = e.status();
        assert_eq!(s.health, Health::Up);
        let info = s.info.unwrap();
        assert_eq!((info.name.as_str(), info.model.as_str()), ("ACE-Step 1.5", "acestep-v15-turbo"));
        assert_eq!(info.models, ["acestep-v15-base", "acestep-v15-turbo"]);
        assert_eq!((info.queued, info.running), (2, 1));
        assert_eq!(info.licence.name, "MIT");
    }

    #[tokio::test]
    async fn an_engine_nothing_listens_for_is_down_after_one_check() {
        let e = engine("http://127.0.0.1:9".into());
        e.check().await;
        assert_eq!(e.status().health, Health::Down, "a refused connection is certain; only timeouts need 3 in a row");
    }

    #[tokio::test]
    async fn a_change_of_health_is_told() {
        let (e, mut rx) = telling("http://127.0.0.1:9".into());
        e.check().await;
        e.check().await;
        assert_eq!(told(&mut rx), 1, "once, when it went down");
    }

    #[tokio::test]
    async fn a_change_in_what_the_engine_reports_is_told_too() {
        let fake = Fake::default();
        *fake.initialized.lock() = true;
        let (e, mut rx) = telling(serve(fake.clone()).await);
        e.check().await;
        e.check().await;
        assert_eq!(told(&mut rx), 1, "up, then nothing new");
        *fake.model.lock() = Some("acestep-v15-base");
        e.check().await;
        assert_eq!(told(&mut rx), 1, "another model, though still up");
    }

    #[tokio::test]
    async fn a_model_switch_is_told_as_loading_at_once() {
        let fake = Fake::default();
        *fake.initialized.lock() = true;
        *fake.init_ms.lock() = 500;
        let (e, mut rx) = telling(serve(fake.clone()).await);
        let e = Arc::new(e);
        e.check().await;
        told(&mut rx);
        let switching = tokio::spawn({
            let e = e.clone();
            async move { e.switch_model("acestep-v15-base").await }
        });
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        assert_eq!(told(&mut rx), 1, "loading, before ACE-Step answers");
        switching.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn a_call_that_cannot_connect_names_the_address_and_marks_the_engine_down() {
        let e = engine("http://127.0.0.1:9".into());
        assert_eq!(e.submit(&request()).await, Err(CallError::Unreachable("127.0.0.1:9".into())));
        assert_eq!(e.status().health, Health::Down, "down at once, not after three health checks");
    }

    #[tokio::test]
    async fn an_answer_that_is_not_json_says_so() {
        let app = Router::new().route("/query_result", post(|| async { "not json" }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        assert_eq!(engine(url).job("t1").await, Err(CallError::Transport("an answer Maio does not understand".into())));
    }

    #[tokio::test]
    async fn a_full_queue_is_busy_not_a_refusal() {
        let fake = Fake::default();
        *fake.busy.lock() = 1;
        let e = engine(serve(fake.clone()).await);
        assert_eq!(e.submit(&request()).await, Err(CallError::Busy));
        assert_eq!(e.submit(&request()).await, Ok("t1".into()), "taken once the queue has room");
    }

    #[tokio::test]
    async fn a_busy_engine_does_not_write_the_lyrics_again() {
        let fake = Fake::default();
        *fake.busy.lock() = 1;
        let e = engine(serve(fake.clone()).await);
        let sung = EngineRequest { vocals: true, write_lyrics: true, ..request() };
        assert_eq!(e.submit(&sung).await, Err(CallError::Busy));
        assert_eq!(e.submit(&sung).await, Ok("t1".into()));
        let posted: Vec<String> = fake.posted.lock().iter().map(|(p, _)| p.clone()).collect();
        assert_eq!(posted, ["/v1/create_sample", "/release_task", "/release_task"], "the lyrics are written once");
        assert_eq!(fake.posted.lock()[2].1["lyrics"], "[verse]\nwritten", "and sent again with the song");
    }

    #[test]
    fn checks_come_every_10_s_and_every_5_s_after_a_failure_or_while_down() {
        assert_eq!(next_check(Health::Up, 0), Duration::from_secs(10));
        assert_eq!(next_check(Health::Loading, 0), Duration::from_secs(10));
        assert_eq!(next_check(Health::Up, 1), Duration::from_secs(5), "a failed check is checked again soon");
        assert_eq!(next_check(Health::Down, 3), Duration::from_secs(5), "back within 5 s of answering again");
    }

    #[tokio::test]
    async fn a_song_is_released_polled_and_downloaded() {
        let fake = Fake::default();
        *fake.status.lock() = 0;
        let e = engine(serve(fake.clone()).await);
        let id = e.submit(&request()).await.unwrap();
        assert_eq!(id, "t1");
        let (path, body) = fake.posted.lock()[0].clone();
        assert_eq!(path, "/release_task");
        assert_eq!(body["prompt"], "calm");
        assert_eq!(body["lyrics"], "[Instrumental]");
        assert_eq!(body["audio_duration"], 30);
        assert_eq!(body["seed"], 7);
        assert_eq!(body["use_random_seed"], false);
        assert_eq!(body["batch_size"], 1);
        assert_eq!(body["audio_format"], "wav");
        assert_eq!(e.job("t1").await.unwrap().state, "running");
        *fake.status.lock() = 1;
        let done = e.job("t1").await.unwrap();
        assert_eq!(done.state, "done");
        assert_eq!(done.lyrics.as_deref(), Some("[verse]\nsung"));
        let metas = done.metas.unwrap();
        assert_eq!((metas.bpm, metas.key.as_deref(), metas.time_signature.as_deref()), (Some(92), Some("A minor"), Some("4")));
        let dir = tempfile::tempdir().unwrap();
        e.download("t1", &dir.path().join("t1.wav")).await.unwrap();
        assert_eq!(std::fs::read(dir.path().join("t1.wav")).unwrap(), b"RIFFfake");
    }

    #[tokio::test]
    async fn ace_step_writes_the_lyrics_when_asked() {
        let fake = Fake::default();
        let e = engine(serve(fake.clone()).await);
        e.submit(&EngineRequest { vocals: true, write_lyrics: true, ..request() }).await.unwrap();
        let posted = fake.posted.lock().clone();
        assert_eq!(posted[0].0, "/v1/create_sample");
        assert_eq!(posted[0].1["query"], "calm");
        assert_eq!(posted[1].1["lyrics"], "[verse]\nwritten");
        assert_eq!(posted[1].1["vocal_language"], "en");
    }

    #[tokio::test]
    async fn a_failed_task_says_so() {
        let fake = Fake::default();
        *fake.status.lock() = 2;
        let e = engine(serve(fake).await);
        let job = e.job("t1").await.unwrap();
        assert_eq!(job.state, "failed");
        assert!(job.error.is_some());
    }

    #[tokio::test]
    async fn a_lost_task_is_not_found() {
        let fake = Fake::default();
        *fake.status.lock() = -1;
        let e = engine(serve(fake).await);
        assert_eq!(e.job("t1").await, Err(CallError::NotFound));
    }

    #[tokio::test]
    async fn switching_model_asks_ace_step_to_load_it() {
        let fake = Fake::default();
        *fake.initialized.lock() = true;
        let e = engine(serve(fake.clone()).await);
        e.check().await;
        e.switch_model("acestep-v15-base").await.unwrap();
        let (path, body) = fake.posted.lock()[0].clone();
        assert_eq!((path.as_str(), body["model"].as_str(), body["slot"].as_u64()), ("/v1/init", Some("acestep-v15-base"), Some(1)));
        assert_eq!(e.status().health, Health::Up, "up again once ACE-Step has loaded it");
    }

    #[tokio::test]
    async fn a_task_being_made_reports_ace_steps_own_progress_and_stage() {
        let fake = Fake::default();
        *fake.status.lock() = 0;
        *fake.stage.lock() = "running";
        let e = engine(serve(fake.clone()).await);
        let job = e.job("t1").await.unwrap();
        assert_eq!((job.state.as_str(), job.progress), ("running", 0.4));
        *fake.stage.lock() = "queued";
        assert_eq!(e.job("t1").await.unwrap().state, "queued", "waiting inside ACE-Step is not being made");
    }

    #[tokio::test]
    async fn the_panel_shows_loading_for_as_long_as_a_switch_takes() {
        let fake = Fake::default();
        *fake.initialized.lock() = true;
        *fake.init_ms.lock() = 500;
        let e = Arc::new(engine(serve(fake.clone()).await));
        e.check().await;
        let switching = tokio::spawn({
            let e = e.clone();
            async move { e.switch_model("acestep-v15-base").await }
        });
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        e.check().await; // the 30 s health check lands mid-switch; ACE-Step still says the old model is up
        assert_eq!(e.status().health, Health::Loading);
        switching.await.unwrap().unwrap();
        e.check().await;
        assert_eq!(e.status().health, Health::Up);
    }

    #[tokio::test]
    async fn a_key_goes_with_every_call_when_ace_step_wants_one() {
        let fake = Fake::default();
        *fake.status.lock() = 1;
        let url = serve(fake.clone()).await;
        let e = HttpEngine::new(
            EngineConfig { id: "ace-step".into(), url, api_key: Some("ace-secret".into()) },
            tokio::sync::mpsc::unbounded_channel().0,
        );
        e.check().await;
        e.submit(&request()).await.unwrap();
        e.job("t1").await.unwrap();
        let auth = fake.auth.lock().clone();
        assert!(auth.len() >= 3, "{auth:?}");
        assert!(auth.iter().all(|a| a == "Bearer ace-secret"), "{auth:?}");
    }
}
