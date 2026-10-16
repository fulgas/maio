use std::{path::PathBuf, time::Duration};

use serde::Serialize;

use crate::{
    engine::{self, CallError, Commercial, Engine, EngineRequest},
    media,
    queue::EntryState,
    request::Normalized,
    state::AppState,
    store::{self, Track},
};

const POLL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    Pending,
    Running,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobStatus {
    pub id: String,
    pub state: JobState,
    pub progress: f32,
    pub track_id: Option<String>,
    pub error: Option<String>,
    /// Set when the engine's licence is `limited` (SPEC 3, Licences).
    pub warning: Option<String>,
    #[serde(skip)]
    pub token: String,
    #[serde(skip)]
    pub entry_id: Option<String>,
    /// The radio whose queue entry this job fills.
    #[serde(skip)]
    pub radio: Option<String>,
    #[serde(skip)]
    pub cancelled: bool,
    /// What happened along the way, for example a lyrics fallback.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

pub fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", ulid::Ulid::generate())
}

/// Expected wait: generation time from the chosen engine's realtime factor, plus 30 s for download and import.
pub fn eta_s(state: &AppState, n: &Normalized) -> Option<u32> {
    let engine = engine::choose(&state.engines, n, &[]).ok()?;
    let info = engine.snapshot().0?;
    Some((info.realtime_factor * n.seconds as f32) as u32 + 30)
}

/// Registers a pending job and runs it in the background.
pub fn start(state: &AppState, job_id: String, n: Normalized, token: &str, radio: Option<&str>, entry_id: Option<String>, playlist_id: Option<String>) {
    {
        let mut jobs = state.jobs.lock().unwrap();
        if jobs.len() > 1000 {
            jobs.retain(|_, j| matches!(j.state, JobState::Pending | JobState::Running));
        }
        jobs.insert(job_id.clone(), JobStatus {
            id: job_id.clone(), state: JobState::Pending, progress: 0.0, track_id: None, error: None, warning: None,
            token: token.into(), entry_id, radio: radio.map(Into::into), cancelled: false, note: None,
        });
    }
    tokio::spawn(run(state.clone(), job_id, n, token.to_string(), playlist_id));
}

/// Marks jobs cancelled. Their runners stop at the next poll and free the engine job.
pub fn cancel(state: &AppState, job_ids: &[String]) {
    let mut jobs = state.jobs.lock().unwrap();
    for id in job_ids {
        if let Some(j) = jobs.get_mut(id) {
            j.cancelled = true;
        }
    }
}

/// Jobs from this token that are not finished (SPEC 2, `pending_jobs`).
pub fn pending_for(state: &AppState, token: &str) -> u32 {
    state.jobs.lock().unwrap().values()
        .filter(|j| j.token == token && !j.cancelled && matches!(j.state, JobState::Pending | JobState::Running))
        .count() as u32
}

struct Failure {
    retryable: bool,
    message: String,
}

async fn run(state: AppState, job_id: String, mut n: Normalized, token: String, playlist_id: Option<String>) {
    // Lyrics (SPEC 10): the request's lyrics_by, else the playlist's (already in the request), else config.
    let mut write_lyrics = false;
    let mut lyrics_engine: Option<String> = None;
    if n.vocals && n.lyrics.is_none() {
        let by = n.lyrics_by.clone().unwrap_or_else(|| state.cfg.lyrics.default_engine.clone());
        match state.text_engine(&by) {
            Some(te) => match crate::text::write_lyrics(&te, &n, crate::text::LYRICS_TIMEOUT).await {
                Ok(l) => {
                    n.lyrics = Some(l);
                    lyrics_engine = Some(by);
                }
                Err(e) => {
                    tracing::warn!(job = %job_id, text_engine = %by, "lyrics: {e:#}");
                    update(&state, &job_id, |j| j.note = Some(format!("lyrics by {by} failed, so ACE-Step wrote them")));
                    write_lyrics = true;
                }
            },
            None => write_lyrics = true,
        }
    }
    let mut exclude: Vec<String> = Vec::new();
    loop {
        let engine = match engine::choose(&state.engines, &n, &exclude) {
            Ok(e) => e,
            Err(e) => return finish_failed(&state, &job_id, e.message, playlist_id.is_some()),
        };
        if let Some(info) = engine.snapshot().0 {
            if info.licence.commercial == Commercial::Limited {
                update(&state, &job_id, |j| j.warning = Some(format!("{} licence: {}", info.licence.name, info.licence.note)));
            }
        }
        match generate_on(&state, &engine, &job_id, &n, write_lyrics).await {
            Ok(None) => return finish_failed(&state, &job_id, "cancelled".into(), false),
            Ok(Some((wav, remote))) => {
                let result = import(&state, &engine, &n, &wav, &remote, write_lyrics, lyrics_engine.clone(), &token, playlist_id.as_deref()).await;
                let _ = tokio::fs::remove_file(&wav).await;
                return match result {
                    Ok(track) => finish_done(&state, &job_id, track, playlist_id.is_some()),
                    Err(e) => finish_failed(&state, &job_id, format!("import: {e:#}"), playlist_id.is_some()),
                };
            }
            Err(f) if f.retryable && exclude.is_empty() => {
                tracing::warn!(job = %job_id, engine = %engine.cfg.id, "retrying elsewhere: {}", f.message);
                exclude.push(engine.cfg.id.clone());
            }
            Err(f) => return finish_failed(&state, &job_id, f.message, playlist_id.is_some()),
        }
    }
}

fn update(state: &AppState, job_id: &str, f: impl FnOnce(&mut JobStatus)) {
    if let Some(j) = state.jobs.lock().unwrap().get_mut(job_id) {
        f(j);
    }
}

fn is_cancelled(state: &AppState, job_id: &str) -> bool {
    state.jobs.lock().unwrap().get(job_id).is_none_or(|j| j.cancelled)
}

async fn generate_on(state: &AppState, engine: &Engine, job_id: &str, n: &Normalized, write_lyrics: bool) -> Result<Option<(PathBuf, engine::RemoteJob)>, Failure> {
    let mut req = EngineRequest::from(n);
    req.write_lyrics = write_lyrics;
    let remote = loop {
        if is_cancelled(state, job_id) {
            return Ok(None);
        }
        match engine.submit(&req).await {
            Ok(id) => break id,
            Err(CallError::Busy) => tokio::time::sleep(Duration::from_secs(5)).await,
            Err(CallError::Rejected(m)) => return Err(Failure { retryable: false, message: m }),
            Err(e) => {
                engine.fail();
                return Err(Failure { retryable: true, message: format!("submit: {e:?}") });
            }
        }
    };
    let mut transport_errors = 0;
    loop {
        tokio::time::sleep(POLL).await;
        if is_cancelled(state, job_id) {
            engine.delete(&remote).await;
            return Ok(None);
        }
        match engine.job(&remote).await {
            Ok(j) => {
                transport_errors = 0;
                match j.state.as_str() {
                    "queued" => {}
                    "running" => update(state, job_id, |s| {
                        s.state = JobState::Running;
                        s.progress = j.progress;
                    }),
                    "done" => {
                        let wav = state.tmp_dir().join(format!("{job_id}.wav"));
                        let got = engine.download(&remote, &wav).await;
                        engine.delete(&remote).await;
                        return got.map(|_| Some((wav, j))).map_err(|e| Failure { retryable: true, message: format!("download: {e:?}") });
                    }
                    "failed" => {
                        let e = j.error.unwrap_or(engine::RemoteError { code: "unknown".into(), message: "no error given".into(), retryable: false });
                        return Err(Failure { retryable: e.retryable, message: format!("{}: {}", e.code, e.message) });
                    }
                    "cancelled" => return Ok(None),
                    other => return Err(Failure { retryable: false, message: format!("unknown job state {other}") }),
                }
            }
            Err(CallError::NotFound) => return Err(Failure { retryable: true, message: "engine lost the job".into() }),
            Err(e) => {
                transport_errors += 1;
                if transport_errors >= 3 {
                    engine.fail();
                    return Err(Failure { retryable: true, message: format!("polling: {e:?}") });
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn import(state: &AppState, engine: &Engine, n: &Normalized, wav: &std::path::Path, remote: &engine::RemoteJob, write_lyrics: bool,
                lyrics_engine: Option<String>, token: &str, playlist_id: Option<&str>) -> anyhow::Result<Track> {
    let id = new_id("trk");
    let file = state.tracks_dir().join(format!("{id}.{}", state.cfg.storage.format.extension()));
    let seconds = media::import(wav, &file, &state.cfg.storage).await?;
    let licence = engine.snapshot().0.map(|i| i.licence);
    let lyrics = remote.lyrics.clone().filter(|l| !l.trim().is_empty()).or_else(|| n.lyrics.clone());
    let lyrics_engine = match (&remote.lyrics, write_lyrics) {
        (Some(_), true) => Some("ace-step".to_string()),
        _ => lyrics_engine,
    };
    let metas = remote.metas.clone().unwrap_or_default();
    let named = crate::titles::name(state, n, lyrics.as_deref()).await;
    let maker = playlist_id.and_then(|p| state.store.playlist(p).ok().flatten()).map(|p| p.name);
    let track = Track {
        id,
        title: named.title.clone(),
        file: file.to_string_lossy().into_owned(),
        prompt: n.prompt.clone(),
        mood: n.mood.clone(),
        genre: n.genre.clone(),
        energy: n.energy.map(|e| e.as_str().to_string()),
        engine: engine.cfg.id.clone(),
        licence: licence.as_ref().map_or_else(|| "unknown".into(), |l| l.name.clone()),
        commercial: licence.map_or_else(|| "unknown".into(), |l| l.commercial.as_str().into()),
        seconds,
        bpm: n.bpm.or(metas.bpm),
        vocals: n.vocals,
        seed: n.seed,
        favourite: false,
        made_by: playlist_id.map(Into::into),
        requested_by: token.into(),
        created_at: store::now(),
        bytes: 0,
        title_source: named.source.into(),
        artist: maker.clone().unwrap_or_else(|| "Maio".into()),
        album: maker.unwrap_or_else(|| "Requests".into()),
        key: metas.key,
        time_signature: metas.time_signature,
        lyrics: lyrics.filter(|_| n.vocals),
        vocal_language: n.vocals.then(|| n.vocal_language.clone()),
        description: named.description.clone(),
        lyrics_engine: lyrics_engine.filter(|_| n.vocals),
        title_engine: named.engine.clone(),
    };
    let mut track = track;
    if let Err(e) = media::write_tags(&file, &media::Tags::from(&track)).await {
        tracing::warn!(track = %track.id, "tagging: {e:#}");
    }
    track.bytes = tokio::fs::metadata(&file).await.map(|m| m.len() as i64).unwrap_or(0);
    state.store.insert_track(&track)?;
    // New tracks join the generative playlist that made them, unless it was deleted meanwhile (SPEC 10).
    if let Some(p) = playlist_id {
        if state.store.playlist(p)?.is_some() {
            state.store.add_to_playlist(p, std::slice::from_ref(&track.id), None)?;
        }
    }
    crate::storage::make_room(state, 0, std::slice::from_ref(&track.id)); // the real size may exceed the estimate
    Ok(track)
}

fn finish_done(state: &AppState, job_id: &str, track: Track, playlist: bool) {
    let mut found = None;
    let mut snapshot = None;
    update(state, job_id, |j| {
        j.state = JobState::Done;
        j.progress = 1.0;
        j.track_id = Some(track.id.clone());
        found = Some((j.entry_id.clone(), j.radio.clone()));
        snapshot = Some(j.clone());
    });
    let (entry_id, radio) = found.unwrap_or_default();
    // A job whose radio was deleted just finishes; its track stays in the crate.
    let r = radio.as_deref().and_then(|id| state.radio(id));
    if let (true, Some(r)) = (playlist, &r) {
        r.backoff.lock().unwrap().succeeded();
    }
    if let (Some(id), Some(r)) = (entry_id, &r) {
        if let Some(e) = r.queue.lock().unwrap().get_mut(&id) {
            if e.state == EntryState::Generating {
                e.title = track.title.clone();
                e.state = EntryState::Ready;
                e.track_id = Some(track.id.clone());
                e.seconds = track.seconds;
            }
        }
        state.emit_queue(r);
    }
    if let Some(j) = snapshot {
        state.emit(radio.as_deref(), "job", serde_json::to_value(j).expect("job serialises"));
    }
}

fn finish_failed(state: &AppState, job_id: &str, message: String, playlist: bool) {
    tracing::warn!(job = %job_id, "job failed: {message}");
    let mut found = None;
    let mut snapshot = None;
    update(state, job_id, |j| {
        j.state = JobState::Failed;
        j.error = Some(message);
        found = Some((j.entry_id.clone(), j.radio.clone()));
        snapshot = Some(j.clone());
    });
    let (entry_id, radio) = found.unwrap_or_default();
    let r = radio.as_deref().and_then(|id| state.radio(id));
    if let (true, Some(r)) = (playlist, &r) {
        r.backoff.lock().unwrap().failed();
    }
    if let (Some(id), Some(r)) = (entry_id, &r) {
        if let Some(e) = r.queue.lock().unwrap().get_mut(&id) {
            if e.state == EntryState::Generating {
                e.state = EntryState::Failed;
            }
        }
        state.emit_queue(r);
    }
    if let Some(j) = snapshot {
        state.emit(radio.as_deref(), "job", serde_json::to_value(j).expect("job serialises"));
    }
}
