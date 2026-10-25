//! Making songs (arc42 8.2): one prompt, one job, one song kept in the library.

use parking_lot::{Mutex, MutexGuard};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use crate::domain::{
    Commercial, Error as Rule, MAX_TITLE, Song, TitleSource,
    engine::{self, EngineInfo, EngineStatus},
    job::{Job, JobState},
    limits::{Rate, Window},
    request::{Normalized, Rules, SongRequest},
    titles,
};
use backon::{ExponentialBuilder, Retryable};
use serde::Serialize;
use tokio::sync::{Notify, Semaphore};
use tracing::Instrument;

use crate::usecases::{
    Error, Result,
    ports::{AudioFiles, CallError, Clock, EngineRequest, Ids, MusicEngine, Notifier, RemoteError, RemoteJob, SongRepository, TextEngine},
};

/// `Retry-After` for a person over `pending_jobs`.
const PENDING_RETRY_S: u64 = 30;
/// Finished jobs kept so a client can read how they ended.
const KEEP_FINISHED: usize = 50;

/// What making songs needs from config (arc42 8.2, 3.4, 7.3).
#[derive(Debug, Clone)]
pub struct MakeSettings {
    pub moods: Vec<String>,
    pub genres: Vec<String>,
    pub default_language: String,
    pub default_lyrics_by: String,
    /// `[naming] engine`: the text engine that names songs, if any.
    pub naming_engine: Option<String>,
    pub make_rate: Rate,
    pub pending_jobs: u32,
    /// How often a running engine job is polled, and the first wait after trouble (arc42 8.8).
    pub poll: Duration,
    /// After a busy answer or a failed poll, each wait doubles up to this (arc42 8.8).
    pub backoff_max: Duration,
    /// arc42 8.2: 30 s for lyrics, 15 s for a title.
    pub lyrics_timeout: Duration,
    pub naming_timeout: Duration,
    /// A job whose engine reports no change in state or progress for this long fails (retryable).
    pub stall: Duration,
}

/// The ports making songs talks to.
pub struct MakingPorts {
    pub engines: Vec<Arc<dyn MusicEngine>>,
    pub text_engines: Vec<Arc<dyn TextEngine>>,
    pub songs: Arc<dyn SongRepository>,
    pub files: Arc<dyn AudioFiles>,
    pub ids: Arc<dyn Ids>,
    pub clock: Arc<dyn Clock>,
    pub notifier: Arc<dyn Notifier>,
}

/// `202 {"job_id", "eta_s"}` (arc42 3.5, `POST /songs`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Accepted {
    pub job_id: String,
    pub eta_s: Option<u32>,
}

struct Entry {
    job: Job,
    cancel: Arc<Notify>,
}

struct Failure {
    retryable: bool,
    message: String,
}

struct Title {
    title: String,
    source: TitleSource,
    engine: Option<String>,
    description: Option<String>,
}

/// A text engine's health, for `/status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TextStatus {
    pub id: String,
    pub healthy: bool,
}

pub struct Making {
    ports: MakingPorts,
    settings: MakeSettings,
    jobs: Mutex<Vec<Entry>>,
    window: Mutex<Window>,
    /// One semaphore per engine, sized by its `concurrency`. Tokio's semaphore is fair, so jobs start in the order asked.
    slots: Mutex<HashMap<String, Arc<Semaphore>>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock()
}

/// Runs a blocking port call (ffmpeg, SQLite) off the async threads.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> anyhow::Result<T> + Send + 'static) -> anyhow::Result<T> {
    tokio::task::spawn_blocking(f).await.map_err(|e| anyhow::anyhow!("blocking task: {e}"))?
}

/// A call to the engine that failed, said so a person knows what to do; the step goes to the log (arc42 8.2).
fn engine_trouble(job: &str, engine: &str, step: &str, e: &CallError) -> Failure {
    tracing::warn!(job = %job, engine = %engine, step, error = %e, "engine call failed");
    let message = match e {
        CallError::Unreachable(address) => {
            format!("the engine {engine} is not answering at {address}; check that it is running, then make the song again")
        }
        e => format!("the engine {engine} failed: {e}"),
    };
    Failure { retryable: true, message }
}

/// Keeps every open job and the most recent finished ones.
fn prune(jobs: &mut Vec<Entry>) {
    let finished = jobs.iter().filter(|e| !e.job.state.is_open()).count();
    let mut extra = finished.saturating_sub(KEEP_FINISHED);
    jobs.retain(|e| {
        if extra > 0 && !e.job.state.is_open() {
            extra -= 1;
            false
        } else {
            true
        }
    });
}

impl Making {
    pub fn new(ports: MakingPorts, settings: MakeSettings) -> Arc<Self> {
        Arc::new(Self { ports, settings, jobs: Mutex::default(), window: Mutex::default(), slots: Mutex::default() })
    }

    /// Checks the request, the limits and the engines, then makes the song in the background.
    pub fn make(self: &Arc<Self>, request: &SongRequest) -> Result<Accepted> {
        let s = &self.settings;
        let text_ids: Vec<String> = self.ports.text_engines.iter().map(|t| t.id().to_string()).collect();
        let rules = Rules {
            moods: &s.moods,
            genres: &s.genres,
            text_engines: &text_ids,
            default_language: &s.default_language,
            default_lyrics_by: &s.default_lyrics_by,
        };
        let n = request.normalize(&rules, self.ports.ids.seed())?;
        let statuses = self.statuses();
        let chosen = engine::choose(&statuses, &n, &[])?;
        let eta_s = statuses[chosen].info.as_ref().map(|i| engine::eta_s(i, n.seconds));
        let label = n.prompt.clone().or_else(|| n.mood.clone()).unwrap_or_default();
        let job = Job::new(self.ports.ids.id(), label, eta_s, self.ports.clock.now());
        {
            let mut jobs = lock(&self.jobs);
            if jobs.iter().filter(|e| e.job.state.is_open()).count() >= s.pending_jobs as usize {
                return Err(Rule::RateLimited { retry_after_s: PENDING_RETRY_S }.into());
            }
            lock(&self.window).allow(s.make_rate, self.ports.clock.epoch_s()).map_err(|retry_after_s| Rule::RateLimited { retry_after_s })?;
            prune(&mut jobs);
            jobs.push(Entry { job: job.clone(), cancel: Arc::new(Notify::new()) });
        }
        tracing::info!(job = %job.id, label = %job.label, "job waiting");
        self.ports.notifier.job(&job);
        // The job's logs and engine calls share a span, a child of the request that asked for it (arc42 8.6).
        let span = tracing::info_span!("job", job = %job.id);
        tokio::spawn(self.clone().run(job.id.clone(), n).instrument(span));
        Ok(Accepted { job_id: job.id, eta_s })
    }

    /// Jobs waiting or making, oldest first (arc42 3.5, `GET /jobs`).
    pub fn jobs(&self) -> Vec<Job> {
        lock(&self.jobs).iter().filter(|e| e.job.state.is_open()).map(|e| e.job.clone()).collect()
    }

    pub fn job(&self, id: &str) -> Result<Job> {
        lock(&self.jobs).iter().find(|e| e.job.id == id).map(|e| e.job.clone()).ok_or_else(|| Error::from(Rule::NotFound("job")))
    }

    pub fn cancel(&self, id: &str) -> Result<Job> {
        let job = {
            let mut jobs = lock(&self.jobs);
            let entry = jobs.iter_mut().find(|e| e.job.id == id).ok_or(Rule::NotFound("job"))?;
            entry.job.cancel()?;
            entry.cancel.notify_one();
            entry.job.clone()
        };
        self.ports.notifier.job(&job);
        Ok(job)
    }

    /// The music engines as last seen (arc42 3.5, `/status`).
    /// The engine panel's model switch (arc42 8.5). The model must be one the engine has installed.
    pub async fn switch_model(&self, engine: &str, model: &str) -> Result<()> {
        let i = self.ports.engines.iter().position(|e| e.status().id == engine).ok_or(Rule::NotFound("engine"))?;
        let installed = self.ports.engines[i].status().info.is_some_and(|info| info.models.iter().any(|m| m == model) || info.models.is_empty());
        if !installed {
            return Err(Rule::invalid("unknown_model", format!("{model:?} is not installed in this engine")).into());
        }
        self.ports.engines[i].switch_model(model).await.map_err(|e| Rule::invalid("switch_failed", e.to_string()).into())
    }

    pub fn engines(&self) -> Vec<EngineStatus> {
        self.statuses()
    }

    pub fn text_engines(&self) -> Vec<TextStatus> {
        self.ports.text_engines.iter().map(|t| TextStatus { id: t.id().to_string(), healthy: t.healthy() }).collect()
    }

    fn statuses(&self) -> Vec<EngineStatus> {
        self.ports.engines.iter().map(|e| e.status()).collect()
    }

    /// Changes an open job and tells the notifier; a cancelled or finished job stays as it is.
    /// The waits after a busy answer or a failed poll: the poll interval, doubling up to `backoff_max`, no jitter
    /// (one client, nothing to spread apart), with no limit on tries unless the caller sets one (arc42 8.8).
    fn backoff(&self) -> ExponentialBuilder {
        ExponentialBuilder::default()
            .with_min_delay(self.settings.poll)
            .with_max_delay(self.settings.backoff_max)
            .with_factor(2.0)
            .without_max_times()
    }

    fn update(&self, id: &str, change: impl FnOnce(&mut Job)) {
        let job = {
            let mut jobs = lock(&self.jobs);
            let Some(entry) = jobs.iter_mut().find(|e| e.job.id == id && e.job.state.is_open()) else { return };
            change(&mut entry.job);
            entry.job.clone()
        };
        self.ports.notifier.job(&job);
    }

    fn is_cancelled(&self, id: &str) -> bool {
        lock(&self.jobs).iter().find(|e| e.job.id == id).is_none_or(|e| e.job.state == JobState::Cancelled)
    }

    fn cancel_signal(&self, id: &str) -> Arc<Notify> {
        lock(&self.jobs).iter().find(|e| e.job.id == id).map(|e| e.cancel.clone()).unwrap_or_default()
    }

    fn slot(&self, engine_id: &str, info: &EngineInfo) -> Arc<Semaphore> {
        let permits = info.capabilities.concurrency.max(1) as usize;
        lock(&self.slots).entry(engine_id.to_string()).or_insert_with(|| Arc::new(Semaphore::new(permits))).clone()
    }

    fn text_engine(&self, id: &str) -> Option<Arc<dyn TextEngine>> {
        self.ports.text_engines.iter().find(|t| t.id() == id && t.healthy()).cloned()
    }

    async fn run(self: Arc<Self>, id: String, mut n: Normalized) {
        match self.produce(&id, &mut n).await {
            Ok(Some(song)) => {
                tracing::info!(job = %id, song = %song.id, "song made");
                self.update(&id, |j| {
                    j.state = JobState::Done;
                    j.progress = 1.0;
                    j.song_id = Some(song.id);
                });
            }
            Ok(None) => tracing::info!(job = %id, "job cancelled"),
            Err(message) => {
                tracing::warn!(job = %id, reason = %message, "job failed");
                self.update(&id, |j| {
                    j.state = JobState::Failed;
                    j.error = Some(message);
                });
            }
        }
    }

    /// The song, `None` when cancelled, or why it failed.
    async fn produce(&self, id: &str, n: &mut Normalized) -> std::result::Result<Option<Song>, String> {
        let (write_lyrics, lyrics_engine) = self.lyrics(id, n).await;
        let mut exclude: Vec<String> = Vec::new();
        // After a retryable failure, the reason a person sees is that failure, not "no other engine".
        let mut first_failure: Option<String> = None;
        loop {
            let statuses = self.statuses();
            let i = match engine::choose(&statuses, n, &exclude) {
                Ok(i) => i,
                Err(e) => return Err(first_failure.unwrap_or_else(|| e.to_string())),
            };
            let (engine, status) = (self.ports.engines[i].clone(), statuses[i].clone());
            let info = status.info.clone().expect("choose only picks engines with info");
            if info.licence.commercial == Commercial::Limited {
                let note = format!("{} licence: {}", info.licence.name, info.licence.note);
                self.update(id, |j| j.licence_note = Some(note));
            }
            match self.generate(id, engine.as_ref(), &status.id, &info, n, write_lyrics).await {
                Ok(None) => return Ok(None),
                Ok(Some((wav, remote))) => {
                    let stored = if self.is_cancelled(id) {
                        Ok(None)
                    } else {
                        self.store(id, &status.id, &info, n, &wav, &remote, write_lyrics, lyrics_engine.clone()).await
                    };
                    let _ = tokio::fs::remove_file(&wav).await;
                    return stored.map_err(|e| format!("storing the song: {e:#}"));
                }
                Err(f) if f.retryable && exclude.is_empty() => {
                    tracing::warn!(job = %id, engine = %status.id, reason = %f.message, "retrying on another engine");
                    exclude.push(status.id);
                    first_failure = Some(f.message);
                }
                Err(f) => return Err(f.message),
            }
        }
    }

    /// With vocals and no lyrics, a text engine writes them, else ACE-Step does (arc42 8.2).
    /// Returns whether the engine must write them, and which text engine did.
    async fn lyrics(&self, id: &str, n: &mut Normalized) -> (bool, Option<String>) {
        if !n.vocals || n.lyrics.is_some() {
            return (false, None);
        }
        if n.lyrics_by == "ace-step" {
            return (true, None);
        }
        let by = n.lyrics_by.clone();
        let written = match self.text_engine(&by) {
            Some(te) => tokio::time::timeout(self.settings.lyrics_timeout, te.write_lyrics(n)).await.ok().and_then(|r| r.ok()),
            None => None,
        };
        match written.map(|l| l.trim().to_string()).filter(|l| !l.is_empty()) {
            Some(lyrics) => {
                n.lyrics = Some(lyrics);
                (false, Some(by))
            }
            None => {
                tracing::warn!(job = %id, text_engine = %by, "lyrics fell back to ACE-Step");
                self.update(id, |j| j.note = Some(format!("lyrics by {by} failed, so ACE-Step wrote them")));
                (true, None)
            }
        }
    }

    /// Waits for a slot, submits, polls, downloads. `None` when cancelled.
    async fn generate(
        &self,
        id: &str,
        engine: &dyn MusicEngine,
        engine_id: &str,
        info: &EngineInfo,
        n: &Normalized,
        write_lyrics: bool,
    ) -> std::result::Result<Option<(PathBuf, RemoteJob)>, Failure> {
        let cancel = self.cancel_signal(id);
        let slot = self.slot(engine_id, info);
        let _permit = tokio::select! {
            permit = slot.acquire_owned() => permit.expect("slots are never closed"),
            () = cancel.notified() => return Ok(None),
        };
        if self.is_cancelled(id) {
            return Ok(None);
        }
        let request = EngineRequest { write_lyrics, ..EngineRequest::from(n) };
        // A busy engine is asked again after backoff, until it takes the song or the song is cancelled.
        let submit = (|| engine.submit(&request)).retry(self.backoff()).when(|e| *e == CallError::Busy);
        let remote = tokio::select! {
            r = submit => match r {
                Ok(remote) => remote,
                Err(CallError::Rejected(m)) => return Err(Failure { retryable: false, message: m }),
                Err(e) => return Err(engine_trouble(id, engine_id, "submit", &e)),
            },
            () = cancel.notified() => return Ok(None),
        };
        let (mut last_seen, mut seen_at) = ((String::new(), -1.0_f32), tokio::time::Instant::now());
        loop {
            // The steady pace between good answers; a failed poll is retried with backoff, 3 tries in all.
            tokio::select! {
                () = tokio::time::sleep(self.settings.poll) => {}
                () = cancel.notified() => {}
            }
            if self.is_cancelled(id) {
                engine.delete(&remote).await;
                return Ok(None);
            }
            let poll = (|| engine.job(&remote)).retry(self.backoff().with_max_times(2)).when(|e| *e != CallError::NotFound);
            let job = match poll.await {
                Ok(job) => job,
                Err(CallError::NotFound) => return Err(Failure { retryable: true, message: "the engine lost the job".into() }),
                Err(e) => return Err(engine_trouble(id, engine_id, "polling", &e)),
            };
            let seen = (job.state.clone(), job.progress);
            if seen != last_seen {
                (last_seen, seen_at) = (seen, tokio::time::Instant::now());
            } else if seen_at.elapsed() >= self.settings.stall {
                engine.delete(&remote).await;
                return Err(Failure { retryable: true, message: "the engine stopped making progress".into() });
            }
            let state = job.state.clone();
            match state.as_str() {
                "queued" => {}
                "running" => self.update(id, |j| {
                    j.state = JobState::Making;
                    j.progress = job.progress.clamp(0.0, 1.0);
                }),
                "done" => {
                    let wav = self.ports.files.scratch(&format!("{id}.wav"));
                    let got = engine.download(&remote, &wav).await;
                    engine.delete(&remote).await;
                    return got.map(|()| Some((wav, job))).map_err(|e| Failure { retryable: true, message: format!("download: {e}") });
                }
                "failed" => {
                    let e = job.error.unwrap_or_else(|| RemoteError { code: "unknown".into(), message: "no reason given".into(), retryable: false });
                    return Err(Failure { retryable: e.retryable, message: format!("{}: {}", e.code, e.message) });
                }
                "cancelled" => return Err(Failure { retryable: false, message: "the engine cancelled the job".into() }),
                other => return Err(Failure { retryable: false, message: format!("unknown job state {other:?}") }),
            }
        }
    }

    /// Imports, names, tags and saves the song. If tagging or saving fails, or the job is cancelled
    /// meanwhile, the file is removed again; `None` when cancelled.
    #[allow(clippy::too_many_arguments)]
    async fn store(
        &self,
        id: &str,
        engine_id: &str,
        info: &EngineInfo,
        n: &Normalized,
        wav: &Path,
        remote: &RemoteJob,
        write_lyrics: bool,
        lyrics_engine: Option<String>,
    ) -> anyhow::Result<Option<Song>> {
        let song_id = self.ports.ids.id();
        let imported = {
            let (files, wav, song_id) = (self.ports.files.clone(), wav.to_path_buf(), song_id.clone());
            blocking(move || files.import(&wav, &song_id)).await?
        };
        let lyrics = remote.lyrics.clone().filter(|l| !l.trim().is_empty()).or_else(|| n.lyrics.clone()).filter(|_| n.vocals);
        let lyrics_engine = match (n.vocals, write_lyrics && remote.lyrics.is_some()) {
            (false, _) => None,
            (true, true) => Some("ace-step".to_string()),
            (true, false) => lyrics_engine,
        };
        let metas = remote.metas.clone().unwrap_or_default();
        let title = self.title(n, lyrics.as_deref()).await;
        let song = Song {
            id: song_id,
            title: title.title,
            title_source: title.source,
            description: title.description,
            prompt: n.prompt.clone().unwrap_or_default(),
            mood: n.mood.clone(),
            genre: n.genre.clone(),
            energy: n.energy.map(|e| e.as_str().to_string()),
            seconds: imported.seconds,
            bpm: n.bpm.or(metas.bpm),
            seed: remote.seed.unwrap_or(n.seed),
            vocals: n.vocals,
            key: metas.key,
            time_signature: metas.time_signature,
            lyrics,
            vocal_language: n.vocals.then(|| n.vocal_language.clone()),
            engine: engine_id.to_string(),
            licence: info.licence.clone(),
            lyrics_engine,
            title_engine: title.engine,
            artist: "Maio".into(),
            file: imported.file,
            bytes: imported.bytes,
            created_at: self.ports.clock.now(),
        };
        if self.is_cancelled(id) {
            let (files, file) = (self.ports.files.clone(), song.file.clone());
            let _ = blocking(move || files.delete(&file)).await;
            return Ok(None);
        }
        let saved = {
            let (files, songs, song) = (self.ports.files.clone(), self.ports.songs.clone(), song.clone());
            blocking(move || {
                files.retag(&song)?;
                songs.insert(&song)
            })
            .await
        };
        if let Err(e) = saved {
            let (files, file) = (self.ports.files.clone(), song.file.clone());
            let _ = blocking(move || files.delete(&file)).await;
            return Err(e);
        }
        self.ports.notifier.song(&song);
        Ok(Some(song))
    }

    /// Titles in layers (arc42 8.2): a text engine, the lyrics, then word lists. A person's rename comes later.
    async fn title(&self, n: &Normalized, lyrics: Option<&str>) -> Title {
        let namer = self.settings.naming_engine.as_deref().and_then(|id| self.text_engine(id));
        if let Some(te) = namer
            && let Ok(Ok(name)) = tokio::time::timeout(self.settings.naming_timeout, te.name_song(n, lyrics)).await
        {
            let title = titles::trim_to(name.title.trim(), MAX_TITLE);
            if !title.is_empty() {
                let description = Some(name.description.trim().to_string()).filter(|d| !d.is_empty());
                return Title { title, source: TitleSource::Engine, engine: Some(te.id().to_string()), description };
            }
        }
        if let Some(title) = lyrics.and_then(titles::from_lyrics) {
            return Title { title, source: TitleSource::Lyrics, engine: None, description: None };
        }
        // ponytail: up to 65 small SQLite lookups on the async thread; move into `blocking` if it ever shows.
        let songs = &self.ports.songs;
        let title = titles::pick_words(n.mood.as_deref(), n.genre.as_deref(), n.seed, |t| songs.title_taken(t).unwrap_or(false));
        Title { title, source: TitleSource::Words, engine: None, description: None }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use crate::domain::engine::Health;

    use super::*;
    use crate::usecases::{
        ports::SongName,
        testing::{FakeEngine, FakeText, FixedClock, MemFiles, MemNotifier, MemSongs, Outcome, SeqIds},
    };

    struct World {
        making: Arc<Making>,
        engines: Vec<Arc<FakeEngine>>,
        songs: Arc<MemSongs>,
        files: Arc<MemFiles>,
        notifier: Arc<MemNotifier>,
    }

    fn settings() -> MakeSettings {
        MakeSettings {
            moods: vec!["calm".into(), "tense".into()],
            genres: vec!["jazz".into()],
            default_language: "en".into(),
            default_lyrics_by: "ace-step".into(),
            naming_engine: None,
            make_rate: Rate { max: 20, window_s: 600 },
            pending_jobs: 4,
            poll: Duration::from_millis(5),
            backoff_max: Duration::from_millis(20),
            lyrics_timeout: Duration::from_millis(100),
            naming_timeout: Duration::from_millis(100),
            stall: Duration::from_secs(10),
        }
    }

    fn world_with(engines: Vec<FakeEngine>, text: Vec<FakeText>, settings: MakeSettings) -> World {
        let engines: Vec<Arc<FakeEngine>> = engines.into_iter().map(Arc::new).collect();
        let (songs, files, notifier) = (Arc::new(MemSongs::default()), Arc::new(MemFiles::default()), Arc::new(MemNotifier::default()));
        let ports = MakingPorts {
            engines: engines.iter().map(|e| e.clone() as Arc<dyn MusicEngine>).collect(),
            text_engines: text.into_iter().map(|t| Arc::new(t) as Arc<dyn TextEngine>).collect(),
            songs: songs.clone(),
            files: files.clone(),
            ids: Arc::new(SeqIds::default()),
            clock: Arc::new(FixedClock("2026-10-16T12:00:00Z")),
            notifier: notifier.clone(),
        };
        World { making: Making::new(ports, settings), engines, songs, files, notifier }
    }

    fn world() -> World {
        world_with(vec![FakeEngine::up("ace-step")], vec![], settings())
    }

    fn calm() -> SongRequest {
        SongRequest { mood: Some("calm".into()), ..Default::default() }
    }

    fn text(id: &str) -> FakeText {
        FakeText {
            id: id.into(),
            healthy: true,
            lyrics: Some("[verse]\nwalking home\n[chorus]\nnight bus to nowhere".into()),
            title: None,
            hang: false,
        }
    }

    /// Waits until the job is in `state`, at most 2 s.
    async fn reaches(w: &World, id: &str, state: JobState) -> Job {
        for _ in 0..400 {
            let job = w.making.job(id).unwrap();
            if job.state == state {
                return job;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("job {id} never reached {state:?}: {:?}", w.making.job(id));
    }

    /// Waits until the job is no longer open, at most 2 s.
    async fn finished(w: &World, id: &str) -> Job {
        for _ in 0..400 {
            let job = w.making.job(id).unwrap();
            if !job.state.is_open() {
                return job;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("job {id} never finished: {:?}", w.making.job(id));
    }

    fn song_of(w: &World, job: &Job) -> Song {
        w.songs.get(job.song_id.as_deref().expect("a done job names its song")).unwrap().expect("the song is stored")
    }

    #[tokio::test]
    async fn a_prompt_makes_one_song_kept_in_the_library() {
        let w = world();
        let accepted = w.making.make(&SongRequest { prompt: Some("warm pads".into()), genre: Some("jazz".into()), ..calm() }).unwrap();
        assert_eq!(accepted.eta_s, Some(120), "0.5 × 180 s + 30 s");
        let job = finished(&w, &accepted.job_id).await;
        assert_eq!(job.state, JobState::Done, "{job:?}");
        let song = song_of(&w, &job);
        assert_eq!((song.prompt.as_str(), song.mood.as_deref(), song.seconds, song.seed), ("warm pads", Some("calm"), 179.5, 1234));
        assert_eq!((song.engine.as_str(), song.artist.as_str(), song.vocals, song.title_source), ("ace-step", "Maio", false, TitleSource::Words));
        assert_eq!(song.file, format!("songs/{}.opus", song.id));
        assert_eq!(w.engines[0].submitted.lock()[0].prompt, "warm pads, jazz, calm mood, instrumental");
        assert_eq!(*w.engines[0].deleted.lock(), ["remote1"], "the engine job is freed");
        assert_eq!(*w.files.retagged.lock(), std::slice::from_ref(&song.title));
        assert_eq!(w.notifier.songs.lock().len(), 1);
        assert!(w.notifier.jobs.lock().iter().any(|j| j.state == JobState::Done));
        assert!(w.making.jobs().is_empty(), "nothing is waiting or making");
    }

    #[tokio::test]
    async fn bad_requests_and_engine_trouble_are_refused_before_a_job_starts() {
        let w = world();
        let code = |r: SongRequest| w.making.make(&r).unwrap_err().rule().map(|e| e.code());
        assert_eq!(code(SongRequest::default()), Some("missing_mood_or_prompt"));
        assert_eq!(code(SongRequest { engine: Some("nope".into()), ..calm() }), Some("unknown_engine"));
        w.engines[0].status.lock().health = Health::Down;
        assert_eq!(code(calm()), Some("engine_unavailable"));
        w.engines[0].status.lock().health = Health::Up;
        w.engines[0].status.lock().info.as_mut().unwrap().licence.commercial = Commercial::No;
        assert_eq!(code(calm()), Some("noncommercial_engine"));
        assert!(w.making.make(&SongRequest { allow_noncommercial: Some(true), ..calm() }).is_ok());
    }

    #[tokio::test]
    async fn pending_jobs_and_the_make_rate_are_limited() {
        let w = world_with(
            vec![FakeEngine::up("ace-step")],
            vec![],
            MakeSettings { pending_jobs: 2, make_rate: Rate { max: 3, window_s: 600 }, ..settings() },
        );
        w.engines[0].hold.store(true, Ordering::SeqCst);
        let first = w.making.make(&calm()).unwrap();
        w.making.make(&calm()).unwrap();
        assert_eq!(w.making.make(&calm()).unwrap_err().rule(), Some(&Rule::RateLimited { retry_after_s: 30 }));
        w.making.cancel(&first.job_id).unwrap();
        w.making.make(&calm()).unwrap();
        w.making.cancel(&w.making.jobs()[0].id).unwrap();
        let e = w.making.make(&calm()).unwrap_err();
        assert_eq!(e.rule(), Some(&Rule::RateLimited { retry_after_s: 600 }), "a fourth in ten minutes waits for the window");
    }

    #[tokio::test]
    async fn jobs_reach_a_one_slot_engine_in_the_order_asked() {
        let w = world();
        w.engines[0].hold.store(true, Ordering::SeqCst);
        let ids: Vec<String> = (1..=3).map(|seed| w.making.make(&SongRequest { seed: Some(seed), ..calm() }).unwrap().job_id).collect();
        reaches(&w, &ids[0], JobState::Making).await;
        assert_eq!(w.making.job(&ids[2]).unwrap().state, JobState::Waiting);
        w.engines[0].hold.store(false, Ordering::SeqCst);
        for id in &ids {
            assert_eq!(finished(&w, id).await.state, JobState::Done);
        }
        let seeds: Vec<i64> = w.engines[0].submitted.lock().iter().map(|r| r.seed).collect();
        assert_eq!(seeds, [1, 2, 3]);
    }

    #[tokio::test]
    async fn a_job_can_be_cancelled_while_waiting_or_making() {
        let w = world();
        w.engines[0].hold.store(true, Ordering::SeqCst);
        let making = w.making.make(&calm()).unwrap().job_id;
        let waiting = w.making.make(&calm()).unwrap().job_id;
        reaches(&w, &making, JobState::Making).await;
        assert_eq!(w.making.job(&waiting).unwrap().state, JobState::Waiting, "one engine slot: the second waits");
        assert_eq!(w.making.cancel(&waiting).unwrap().state, JobState::Cancelled);
        w.making.cancel(&making).unwrap();
        for _ in 0..400 {
            if !w.engines[0].deleted.lock().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(*w.engines[0].deleted.lock(), ["remote1"], "the running engine job is cancelled too");
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert_eq!(w.engines[0].submitted.lock().len(), 1, "the waiting job never reached the engine");
        assert_eq!(w.making.cancel(&making).unwrap_err().rule().map(|e| e.code()), Some("job_finished"));
        assert_eq!(w.making.cancel("nope").unwrap_err().rule(), Some(&Rule::NotFound("job")));
        assert!(w.songs.0.lock().is_empty());
    }

    #[tokio::test]
    async fn a_lost_job_is_retried_once_on_the_next_engine() {
        let w = world_with(vec![FakeEngine::up("first"), FakeEngine::up("second")], vec![], settings());
        *w.engines[0].outcome.lock() = Outcome::Lost;
        let job = finished(&w, &w.making.make(&calm()).unwrap().job_id).await;
        assert_eq!(job.state, JobState::Done, "{job:?}");
        assert_eq!(song_of(&w, &job).engine, "second");
    }

    #[tokio::test]
    async fn a_failed_job_says_why_and_is_retried_only_when_retryable() {
        let w = world_with(vec![FakeEngine::up("first"), FakeEngine::up("second")], vec![], settings());
        *w.engines[0].outcome.lock() = Outcome::Failed { retryable: false };
        let job = finished(&w, &w.making.make(&calm()).unwrap().job_id).await;
        assert_eq!((job.state, job.error.as_deref()), (JobState::Failed, Some("oom: out of memory")));
        assert!(w.engines[1].submitted.lock().is_empty());
        for e in &w.engines {
            *e.outcome.lock() = Outcome::Lost;
        }
        let job = finished(&w, &w.making.make(&calm()).unwrap().job_id).await;
        assert_eq!((job.state, job.error.as_deref()), (JobState::Failed, Some("the engine lost the job")), "one retry, then failed");
        *w.engines[0].outcome.lock() = Outcome::Rejected;
        let job = finished(&w, &w.making.make(&calm()).unwrap().job_id).await;
        assert_eq!((job.state, job.error.as_deref()), (JobState::Failed, Some("422: bad request")));
    }

    #[tokio::test]
    async fn an_engine_that_cannot_be_reached_fails_the_job_with_what_to_do() {
        let w = world_with(vec![FakeEngine::up("ace-step")], vec![], settings());
        *w.engines[0].outcome.lock() = Outcome::Unreachable;
        let job = finished(&w, &w.making.make(&calm()).unwrap().job_id).await;
        assert_eq!(
            (job.state, job.error.as_deref()),
            (JobState::Failed, Some("the engine ace-step is not answering at 127.0.0.1:9; check that it is running, then make the song again"))
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_busy_engine_is_asked_again_after_waits_that_double_up_to_the_most() {
        let w = world_with(vec![FakeEngine::up("ace-step")], vec![], settings());
        w.engines[0].busy.store(4, Ordering::SeqCst);
        finished(&w, &w.making.make(&calm()).unwrap().job_id).await;
        assert_eq!(w.engines[0].gaps_ms("submit"), [5, 10, 20, 20], "poll 5 ms, the most 20 ms");
    }

    #[tokio::test(start_paused = true)]
    async fn failed_polls_wait_longer_each_time_and_a_good_answer_brings_the_steady_poll_back() {
        let w = world_with(vec![FakeEngine::up("ace-step")], vec![], settings());
        w.engines[0].hold.store(true, Ordering::SeqCst);
        w.engines[0].poll_failures.store(2, Ordering::SeqCst);
        let id = w.making.make(&calm()).unwrap().job_id;
        reaches(&w, &id, JobState::Making).await;
        tokio::time::sleep(Duration::from_millis(60)).await;
        assert_eq!(
            w.engines[0].gaps_ms("poll")[..4],
            [5, 10, 5, 5],
            "after each failure the wait doubles, then a good answer brings the steady poll back"
        );
    }

    #[tokio::test]
    async fn vocals_get_lyrics_from_a_text_engine_and_a_title_from_them() {
        let w = world_with(vec![FakeEngine::up("ace-step")], vec![text("local")], settings());
        let job = finished(&w, &w.making.make(&SongRequest { vocals: Some(true), lyrics_by: Some("local".into()), ..calm() }).unwrap().job_id).await;
        let song = song_of(&w, &job);
        let sent = w.engines[0].submitted.lock()[0].clone();
        assert!(!sent.write_lyrics && sent.lyrics.as_deref().is_some_and(|l| l.contains("night bus")));
        assert_eq!((song.lyrics_engine.as_deref(), song.vocal_language.as_deref()), (Some("local"), Some("en")));
        assert_eq!((song.title.as_str(), song.title_source), ("Night Bus To Nowhere", TitleSource::Lyrics));
        assert_eq!(job.note, None);
    }

    #[tokio::test]
    async fn when_the_text_engine_fails_ace_step_writes_the_lyrics() {
        let broken =
            [FakeText { lyrics: None, ..text("local") }, FakeText { hang: true, ..text("local") }, FakeText { healthy: false, ..text("local") }];
        for text_engine in broken {
            let w = world_with(vec![FakeEngine::up("ace-step")], vec![text_engine], settings());
            *w.engines[0].lyrics.lock() = Some("[chorus]\nace wrote this".into());
            let job =
                finished(&w, &w.making.make(&SongRequest { vocals: Some(true), lyrics_by: Some("local".into()), ..calm() }).unwrap().job_id).await;
            assert_eq!(job.note.as_deref(), Some("lyrics by local failed, so ACE-Step wrote them"));
            assert!(w.engines[0].submitted.lock()[0].write_lyrics);
            let song = song_of(&w, &job);
            assert_eq!((song.lyrics.as_deref(), song.lyrics_engine.as_deref()), (Some("[chorus]\nace wrote this"), Some("ace-step")));
        }
    }

    #[tokio::test]
    async fn the_naming_engine_titles_songs_and_word_titles_never_repeat() {
        let namer =
            FakeText { title: Some(SongName { title: "  Harbour Lights ".into(), description: "A slow walk by the sea.".into() }), ..text("namer") };
        let w = world_with(vec![FakeEngine::up("ace-step")], vec![namer], MakeSettings { naming_engine: Some("namer".into()), ..settings() });
        let song = song_of(&w, &finished(&w, &w.making.make(&calm()).unwrap().job_id).await);
        assert_eq!((song.title.as_str(), song.title_source, song.title_engine.as_deref()), ("Harbour Lights", TitleSource::Engine, Some("namer")));
        assert_eq!(song.description.as_deref(), Some("A slow walk by the sea."));

        let w = world();
        let first = song_of(&w, &finished(&w, &w.making.make(&SongRequest { seed: Some(5), ..calm() }).unwrap().job_id).await);
        let second = song_of(&w, &finished(&w, &w.making.make(&SongRequest { seed: Some(5), ..calm() }).unwrap().job_id).await);
        assert_ne!(first.title, second.title, "the same seed moves on to a title the library does not have");
    }

    #[tokio::test]
    async fn a_limited_licence_travels_with_the_job_and_the_song() {
        let w = world();
        {
            let mut status = w.engines[0].status.lock();
            let licence = &mut status.info.as_mut().unwrap().licence;
            licence.commercial = Commercial::Limited;
            licence.name = "SAO".into();
            licence.note = "revenue under $1M".into();
        }
        let job = finished(&w, &w.making.make(&calm()).unwrap().job_id).await;
        assert_eq!(job.licence_note.as_deref(), Some("SAO licence: revenue under $1M"));
        assert_eq!(song_of(&w, &job).licence.note, "revenue under $1M");
    }

    #[tokio::test]
    async fn when_storing_fails_the_file_goes_and_no_song_is_kept() {
        let w = world();
        w.files.fail_retag.store(true, Ordering::SeqCst);
        let job = finished(&w, &w.making.make(&calm()).unwrap().job_id).await;
        assert_eq!(job.state, JobState::Failed);
        assert!(job.error.as_deref().is_some_and(|e| e.starts_with("storing the song")), "{job:?}");
        let imported = w.files.imported.lock()[0].clone();
        assert_eq!(*w.files.deleted.lock(), [format!("songs/{imported}.opus")]);
        assert!(w.songs.0.lock().is_empty());
    }

    #[tokio::test]
    async fn a_job_cancelled_after_the_engine_finished_leaves_no_song() {
        let namer = FakeText { hang: true, ..text("namer") };
        let w = world_with(
            vec![FakeEngine::up("ace-step")],
            vec![namer],
            MakeSettings { naming_engine: Some("namer".into()), naming_timeout: Duration::from_millis(300), ..settings() },
        );
        let id = w.making.make(&calm()).unwrap().job_id;
        for _ in 0..400 {
            if !w.files.imported.lock().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        w.making.cancel(&id).unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(w.songs.0.lock().is_empty(), "a cancelled job keeps no song");
        assert!(w.notifier.songs.lock().is_empty());
        let imported = w.files.imported.lock()[0].clone();
        assert_eq!(*w.files.deleted.lock(), [format!("songs/{imported}.opus")], "and its file goes");
        assert_eq!(w.making.job(&id).unwrap().state, JobState::Cancelled);
    }

    #[tokio::test]
    async fn with_one_engine_a_retryable_failure_keeps_its_own_reason() {
        let w = world();
        *w.engines[0].outcome.lock() = Outcome::Lost;
        let job = finished(&w, &w.making.make(&calm()).unwrap().job_id).await;
        assert_eq!((job.state, job.error.as_deref()), (JobState::Failed, Some("the engine lost the job")));
    }

    #[tokio::test]
    async fn an_engine_that_stops_making_progress_fails_the_job() {
        let w = world_with(vec![FakeEngine::up("ace-step")], vec![], MakeSettings { stall: Duration::from_millis(60), ..settings() });
        w.engines[0].hold.store(true, Ordering::SeqCst);
        let job = finished(&w, &w.making.make(&calm()).unwrap().job_id).await;
        assert_eq!((job.state, job.error.as_deref()), (JobState::Failed, Some("the engine stopped making progress")));
        assert_eq!(*w.engines[0].deleted.lock(), ["remote1"], "the stuck engine job is cancelled");
    }

    #[tokio::test]
    async fn engines_and_text_engines_report_their_health() {
        let w = world_with(vec![FakeEngine::up("ace-step")], vec![FakeText { healthy: false, ..text("local") }], settings());
        assert_eq!(w.making.engines()[0].health, Health::Up);
        assert_eq!(w.making.text_engines(), [TextStatus { id: "local".into(), healthy: false }]);
    }
}
