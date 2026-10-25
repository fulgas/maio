//! In-memory ports for tests, here and in the web layer.

use parking_lot::Mutex;
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

use crate::domain::{
    Commercial, DownloadFormat, Playlist, PlaylistRef, Song,
    engine::{EngineStatus, Health},
    job::Job,
    request::Normalized,
};
use anyhow::{Result, anyhow, bail};
use async_trait::async_trait;

use crate::usecases::ports::{
    AccountStore, AudioFiles, CallError, Clock, EngineRequest, Exported, Ids, Imported, MusicEngine, Notifier, Passwords, PlaylistRepository,
    RemoteError, RemoteJob, SongName, SongQuery, SongRepository, SongSort, TextEngine,
};

#[derive(Default)]
pub struct MemSongs(pub Mutex<Vec<Song>>);

impl SongRepository for MemSongs {
    fn total_bytes(&self) -> Result<u64> {
        Ok(self.0.lock().iter().map(|s| s.bytes).sum())
    }

    fn title_taken(&self, title: &str) -> Result<bool> {
        Ok(self.0.lock().iter().any(|s| s.title == title))
    }

    fn insert(&self, song: &Song) -> Result<()> {
        self.0.lock().push(song.clone());
        Ok(())
    }

    fn get(&self, id: &str) -> Result<Option<Song>> {
        Ok(self.0.lock().iter().find(|s| s.id == id).cloned())
    }

    fn get_many(&self, ids: &[String]) -> Result<Vec<Song>> {
        let all = self.0.lock();
        Ok(ids.iter().filter_map(|id| all.iter().find(|s| &s.id == id).cloned()).collect())
    }

    fn update(&self, song: &Song) -> Result<()> {
        let mut all = self.0.lock();
        if let Some(s) = all.iter_mut().find(|s| s.id == song.id) {
            *s = song.clone();
        }
        Ok(())
    }

    fn delete(&self, id: &str) -> Result<()> {
        self.0.lock().retain(|s| s.id != id);
        Ok(())
    }

    fn search(&self, query: &SongQuery) -> Result<Vec<Song>> {
        let q = query.q.as_deref().map(str::to_lowercase);
        let hit = |s: &Song| match &q {
            None => true,
            Some(q) => [Some(&s.title), Some(&s.prompt), s.mood.as_ref(), s.genre.as_ref(), s.lyrics.as_ref()]
                .into_iter()
                .flatten()
                .any(|f| f.to_lowercase().contains(q)),
        };
        let mut found: Vec<Song> = self.0.lock().iter().filter(|s| hit(s)).cloned().collect();
        match query.sort {
            SongSort::Newest => found.sort_by(|a, b| (&b.created_at, &b.id).cmp(&(&a.created_at, &a.id))),
            SongSort::Title => found.sort_by(|a, b| (a.title.to_lowercase(), &a.id).cmp(&(b.title.to_lowercase(), &b.id))),
        }
        Ok(found.into_iter().skip(query.offset).take(query.limit).collect())
    }
}

#[derive(Default)]
pub struct MemPlaylists(pub Mutex<Vec<Playlist>>);

impl PlaylistRepository for MemPlaylists {
    fn insert(&self, playlist: &Playlist) -> Result<()> {
        self.0.lock().push(playlist.clone());
        Ok(())
    }

    fn get(&self, id: &str) -> Result<Option<Playlist>> {
        Ok(self.0.lock().iter().find(|p| p.id == id).cloned())
    }

    fn save(&self, playlist: &Playlist) -> Result<()> {
        let mut all = self.0.lock();
        if let Some(p) = all.iter_mut().find(|p| p.id == playlist.id) {
            *p = playlist.clone();
        }
        Ok(())
    }

    fn delete(&self, id: &str) -> Result<()> {
        self.0.lock().retain(|p| p.id != id);
        Ok(())
    }

    fn list(&self) -> Result<Vec<Playlist>> {
        Ok(self.0.lock().clone())
    }

    fn containing(&self, song_id: &str) -> Result<Vec<PlaylistRef>> {
        let all = self.0.lock();
        Ok(all.iter().filter(|p| p.song_ids.iter().any(|s| s == song_id)).map(Playlist::to_ref).collect())
    }
}

/// Records what it was asked to do; `import` makes no file.
#[derive(Default)]
pub struct MemFiles {
    pub imported: Mutex<Vec<String>>,
    pub retagged: Mutex<Vec<String>>,
    pub deleted: Mutex<Vec<String>>,
    /// Makes `retag` fail, to test what happens after import.
    pub fail_retag: AtomicBool,
    pub exported: Mutex<Vec<String>>,
    /// Makes `export` hand back a stored file that is no longer there.
    pub vanished: AtomicBool,
}

impl AudioFiles for MemFiles {
    fn scratch(&self, name: &str) -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        std::env::temp_dir().join(format!("maio-test-{}-{}-{name}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)))
    }

    fn import(&self, _wav: &Path, song_id: &str) -> Result<Imported> {
        self.imported.lock().push(song_id.to_string());
        Ok(Imported { file: format!("songs/{song_id}.opus"), seconds: 179.5, bytes: 2000 })
    }

    fn retag(&self, song: &Song) -> Result<()> {
        if self.fail_retag.load(Ordering::SeqCst) {
            bail!("tagging failed");
        }
        self.retagged.lock().push(song.title.clone());
        Ok(())
    }

    fn export(&self, song: &Song, format: DownloadFormat) -> Result<Exported> {
        self.exported.lock().push(format!("{} as {}", song.id, format.extension()));
        if self.vanished.load(Ordering::SeqCst) {
            return Ok(Exported { path: self.scratch("gone.opus"), temporary: false });
        }
        let path = self.scratch(&format!("{}.{}", song.id, format.extension()));
        std::fs::write(&path, format!("{} as {}", song.id, format.extension()))?;
        Ok(Exported { path, temporary: true })
    }

    fn delete(&self, file: &str) -> Result<()> {
        self.deleted.lock().push(file.to_string());
        Ok(())
    }
}

pub struct FixedClock(pub &'static str);

impl Clock for FixedClock {
    fn now(&self) -> String {
        self.0.to_string()
    }

    fn epoch_s(&self) -> u64 {
        1_000_000
    }
}

/// `id1`, `id2`, ...; keys are the same number padded to 32 characters.
#[derive(Default)]
pub struct SeqIds(AtomicUsize);

impl Ids for SeqIds {
    fn id(&self) -> String {
        format!("id{}", self.0.fetch_add(1, Ordering::SeqCst) + 1)
    }

    fn key(&self) -> String {
        format!("{:0>32}", self.0.fetch_add(1, Ordering::SeqCst) + 1)
    }

    fn seed(&self) -> i64 {
        1234
    }

    fn secret(&self) -> String {
        format!("{:0>64}", self.0.fetch_add(1, Ordering::SeqCst) + 1)
    }
}

#[derive(Default)]
pub struct MemNotifier {
    pub jobs: Mutex<Vec<Job>>,
    pub songs: Mutex<Vec<Song>>,
}

impl Notifier for MemNotifier {
    fn job(&self, job: &Job) {
        self.jobs.lock().push(job.clone());
    }

    fn song(&self, song: &Song) {
        self.songs.lock().push(song.clone());
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Outcome {
    Done,
    Failed {
        retryable: bool,
    },
    /// The engine forgets the job (`404`).
    Lost,
    /// `POST /jobs` is refused.
    Rejected,
    /// Nothing answers at the engine's address.
    Unreachable,
}

/// An engine that finishes every job on the first poll, unless told otherwise.
pub struct FakeEngine {
    pub status: Mutex<EngineStatus>,
    pub outcome: Mutex<Outcome>,
    /// Lyrics a finished job reports, as ACE-Step does when it wrote them.
    pub lyrics: Mutex<Option<String>>,
    /// While set, every job stays running at 0.5.
    pub hold: AtomicBool,
    pub submitted: Mutex<Vec<EngineRequest>>,
    pub deleted: Mutex<Vec<String>>,
    /// Models the engine panel asked it to load.
    pub switched: Mutex<Vec<String>>,
    /// Answers this many submits with `429`, then accepts.
    pub busy: AtomicUsize,
    /// Fails this many polls with no answer, then answers.
    pub poll_failures: AtomicUsize,
    /// When each submit and each poll came, in Tokio's (pausable) time.
    pub calls: Mutex<Vec<(&'static str, tokio::time::Instant)>>,
}

impl FakeEngine {
    /// Healthy, MIT, 10 to 600 s, vocals, one slot, half real time.
    pub fn up(id: &str) -> Self {
        let info = crate::domain::testing::engine_info(id, Commercial::Yes);
        Self {
            status: Mutex::new(EngineStatus { id: id.into(), info: Some(info), health: Health::Up }),
            outcome: Mutex::new(Outcome::Done),
            lyrics: Mutex::new(None),
            hold: AtomicBool::new(false),
            submitted: Mutex::default(),
            deleted: Mutex::default(),
            switched: Mutex::default(),
            busy: AtomicUsize::new(0),
            poll_failures: AtomicUsize::new(0),
            calls: Mutex::default(),
        }
    }

    /// The gaps between this engine's calls of one kind, in milliseconds.
    pub fn gaps_ms(&self, kind: &str) -> Vec<u128> {
        let at: Vec<_> = self.calls.lock().iter().filter(|(k, _)| *k == kind).map(|(_, t)| *t).collect();
        at.windows(2).map(|w| (w[1] - w[0]).as_millis()).collect()
    }
}

#[async_trait]
impl MusicEngine for FakeEngine {
    fn status(&self) -> EngineStatus {
        self.status.lock().clone()
    }

    async fn submit(&self, request: &EngineRequest) -> std::result::Result<String, CallError> {
        self.calls.lock().push(("submit", tokio::time::Instant::now()));
        if self.busy.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1)).is_ok() {
            return Err(CallError::Busy);
        }
        match *self.outcome.lock() {
            Outcome::Rejected => return Err(CallError::Rejected("422: bad request".into())),
            Outcome::Unreachable => return Err(CallError::Unreachable("127.0.0.1:9".into())),
            _ => {}
        }
        let mut submitted = self.submitted.lock();
        submitted.push(request.clone());
        Ok(format!("remote{}", submitted.len()))
    }

    async fn job(&self, id: &str) -> std::result::Result<RemoteJob, CallError> {
        self.calls.lock().push(("poll", tokio::time::Instant::now()));
        if self.poll_failures.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1)).is_ok() {
            return Err(CallError::Transport("no answer in time".into()));
        }
        let running = RemoteJob { job_id: id.into(), state: "running".into(), progress: 0.5, ..Default::default() };
        if self.hold.load(Ordering::SeqCst) {
            return Ok(running);
        }
        let seed = self.submitted.lock().last().map(|r| r.seed);
        let outcome = *self.outcome.lock();
        match outcome {
            Outcome::Done => {
                let lyrics = self.lyrics.lock().clone();
                Ok(RemoteJob { state: "done".into(), progress: 1.0, seconds: Some(179.8), seed, lyrics, ..running })
            }
            Outcome::Failed { retryable } => {
                let error = RemoteError { code: "oom".into(), message: "out of memory".into(), retryable };
                Ok(RemoteJob { state: "failed".into(), error: Some(error), ..running })
            }
            Outcome::Lost => Err(CallError::NotFound),
            Outcome::Rejected | Outcome::Unreachable => unreachable!("a job the engine never took is never polled"),
        }
    }

    async fn download(&self, _id: &str, to: &Path) -> std::result::Result<(), CallError> {
        std::fs::write(to, b"RIFF").map_err(|e| CallError::Transport(e.to_string()))
    }

    async fn delete(&self, id: &str) {
        self.deleted.lock().push(id.into());
    }

    async fn switch_model(&self, model: &str) -> std::result::Result<(), CallError> {
        self.switched.lock().push(model.to_string());
        Ok(())
    }
}

/// A text engine with canned answers; `None` answers fail, `hang` never answers.
pub struct FakeText {
    pub id: String,
    pub healthy: bool,
    pub lyrics: Option<String>,
    pub title: Option<SongName>,
    pub hang: bool,
}

#[async_trait]
impl TextEngine for FakeText {
    fn id(&self) -> &str {
        &self.id
    }

    fn healthy(&self) -> bool {
        self.healthy
    }

    async fn write_lyrics(&self, _n: &Normalized) -> Result<String> {
        if self.hang {
            std::future::pending::<()>().await;
        }
        self.lyrics.clone().ok_or_else(|| anyhow!("no lyrics"))
    }

    async fn name_song(&self, _n: &Normalized, _lyrics: Option<&str>) -> Result<SongName> {
        if self.hang {
            std::future::pending::<()>().await;
        }
        self.title.clone().ok_or_else(|| anyhow!("no title"))
    }
}

#[derive(Default)]
pub struct MemAccount {
    pub account: Mutex<Option<(String, String)>>,
    pub sessions: Mutex<Vec<(String, u64)>>,
}

impl AccountStore for MemAccount {
    fn account(&self) -> Result<Option<(String, String)>> {
        Ok(self.account.lock().clone())
    }

    fn set_account(&self, username: &str, hash: &str) -> Result<()> {
        *self.account.lock() = Some((username.into(), hash.into()));
        Ok(())
    }

    fn create_session(&self, id: &str, expires_at: u64) -> Result<()> {
        self.sessions.lock().push((id.into(), expires_at));
        Ok(())
    }

    fn session_valid(&self, id: &str, now: u64) -> Result<bool> {
        Ok(self.sessions.lock().iter().any(|(s, exp)| s == id && *exp > now))
    }

    fn delete_session(&self, id: &str) -> Result<()> {
        self.sessions.lock().retain(|(s, _)| s != id);
        Ok(())
    }

    fn delete_other_sessions(&self, keep: &str) -> Result<()> {
        self.sessions.lock().retain(|(s, _)| s == keep);
        Ok(())
    }

    fn delete_expired_sessions(&self, now: u64) -> Result<()> {
        self.sessions.lock().retain(|(_, exp)| *exp > now);
        Ok(())
    }
}

/// "Hashes" by prefixing, so tests stay fast; counts the checks spent on unknown usernames.
#[derive(Default)]
pub struct PlainPasswords {
    pub dummy_checks: AtomicUsize,
}

impl Passwords for PlainPasswords {
    fn hash(&self, password: &str) -> Result<String> {
        Ok(format!("plain:{password}"))
    }

    fn verify(&self, password: &str, hash: &str) -> bool {
        hash == format!("plain:{password}")
    }

    fn verify_nothing(&self, _password: &str) {
        self.dummy_checks.fetch_add(1, Ordering::SeqCst);
    }
}

/// Records what the use cases ask of the live streams.
#[derive(Default)]
pub struct FakeLive {
    pub calls: Mutex<Vec<String>>,
    pub on_air: Mutex<std::collections::HashMap<String, crate::usecases::ports::OnAir>>,
}

impl crate::usecases::ports::LiveStreams for FakeLive {
    fn play(&self, p: &crate::domain::Playlist, songs: &[crate::usecases::ports::LiveSong]) {
        let ids: Vec<&str> = songs.iter().map(|s| s.id.as_str()).collect();
        self.calls.lock().push(format!("play {} [{}] {}", p.id, ids.join(","), p.live.visibility.as_str()));
    }

    fn stop(&self, id: &str) {
        self.calls.lock().push(format!("stop {id}"));
    }

    fn on_air(&self, id: &str) -> Option<crate::usecases::ports::OnAir> {
        self.on_air.lock().get(id).cloned()
    }
}

/// Live streams as listeners see them: every stream exists; asks are recorded.
#[derive(Default)]
pub struct FakeListening {
    pub asked: Mutex<Vec<String>>,
}

impl crate::usecases::ports::LiveListening for FakeListening {
    fn listen(&self, id: &str, format: crate::domain::live::StreamFormat) -> Option<crate::usecases::ports::Listen> {
        self.asked.lock().push(format!("{id} {format:?}"));
        let (_tx, bytes) = tokio::sync::broadcast::channel(1);
        let (_moved_tx, moved) = tokio::sync::watch::channel(0);
        Some(crate::usecases::ports::Listen { bytes, moved, guard: Box::new(()) })
    }

    fn now_title(&self, _id: &str) -> Option<String> {
        None
    }

    fn hls_dir(&self, id: &str) -> PathBuf {
        PathBuf::from(format!("/hls/{id}"))
    }
}

/// MediaMTX with the secret `secret`, or no relay at all (`default`).
#[derive(Default)]
pub struct FakeRelay {
    on: bool,
}

impl FakeRelay {
    pub fn on() -> Self {
        Self { on: true }
    }
}

impl crate::usecases::ports::Relay for FakeRelay {
    fn caller(&self, user: &str, password: &str) -> Option<crate::domain::relay::Caller> {
        use crate::domain::relay::Caller;
        self.on.then(|| match (user, password) {
            ("maio", "secret") => Caller::Maio,
            ("maio", _) => Caller::Impostor,
            _ => Caller::Anyone,
        })
    }

    fn status(&self, live: &[(PlaylistRef, String)]) -> Option<crate::domain::relay::RelayStatus> {
        use crate::domain::relay::{RelayReaders, RelayState, RelayStatus, RelayStream};
        self.on.then(|| RelayStatus {
            state: RelayState::Up,
            version: None,
            up_s: None,
            rtsp: String::new(),
            srt: String::new(),
            rtmp: String::new(),
            streams: live
                .iter()
                .map(|(playlist, _)| RelayStream {
                    playlist: playlist.clone(),
                    arriving: true,
                    arriving_s: None,
                    format: None,
                    readers: RelayReaders::default(),
                    bytes_sent: 0,
                    frames_in_error: 0,
                })
                .collect(),
        })
    }

    fn addresses(&self, path: &str) -> Option<crate::domain::relay::RelayAddresses> {
        self.on.then(|| crate::domain::relay::RelayAddresses { rtsp: format!("rtsp://relay/{path}"), srt: String::new(), rtmp: String::new() })
    }
}
