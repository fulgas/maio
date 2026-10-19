//! The ports: small traits the use cases need and adapters implement (SPEC 3).

use std::{
    fmt,
    path::{Path, PathBuf},
};

use anyhow::Result;
use async_trait::async_trait;
use domain::{Playlist, PlaylistRef, Song, engine::EngineStatus, job::Job, request::Normalized};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SongSort {
    #[default]
    Newest,
    Title,
}

/// One page of songs: `q` matches titles, prompts, moods, genres and lyrics (SPEC 10, `GET /songs`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SongQuery {
    pub q: Option<String>,
    pub sort: SongSort,
    pub limit: usize,
    pub offset: usize,
}

pub trait SongRepository: Send + Sync {
    fn insert(&self, song: &Song) -> Result<()>;
    fn get(&self, id: &str) -> Result<Option<Song>>;
    /// The songs with these ids, in the order given; unknown ids are left out.
    fn get_many(&self, ids: &[String]) -> Result<Vec<Song>>;
    fn update(&self, song: &Song) -> Result<()>;
    fn delete(&self, id: &str) -> Result<()>;
    fn search(&self, query: &SongQuery) -> Result<Vec<Song>>;
    /// Whether a song already has exactly this title (SPEC 5, layer 4).
    fn title_taken(&self, title: &str) -> Result<bool>;
}

pub trait PlaylistRepository: Send + Sync {
    fn insert(&self, playlist: &Playlist) -> Result<()>;
    fn get(&self, id: &str) -> Result<Option<Playlist>>;
    /// Stores the name, the live settings and the songs in order.
    fn save(&self, playlist: &Playlist) -> Result<()>;
    fn delete(&self, id: &str) -> Result<()>;
    /// Every playlist, oldest first.
    fn list(&self) -> Result<Vec<Playlist>>;
    /// The playlists that hold this song, oldest first.
    fn containing(&self, song_id: &str) -> Result<Vec<PlaylistRef>>;
}

pub trait AudioFiles: Send + Sync {
    /// A path for a temporary file, such as an engine's WAV before import.
    fn scratch(&self, name: &str) -> PathBuf;
    /// Transcodes and loudness-normalises `wav` into the song's stored file (SPEC 5).
    fn import(&self, wav: &Path, song_id: &str) -> Result<Imported>;
    /// Rewrites the file's tags from the song, without re-encoding (SPEC 5).
    fn retag(&self, song: &Song) -> Result<()>;
    /// `file` is relative to the data folder, as `Song::file`.
    fn delete(&self, file: &str) -> Result<()>;
}

/// A stored audio file, fresh from import.
#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    /// Relative to the data folder: `songs/{id}.{ext}`.
    pub file: String,
    pub seconds: f64,
    pub bytes: u64,
}

pub trait Clock: Send + Sync {
    /// UTC, `YYYY-MM-DDTHH:MM:SSZ`.
    fn now(&self) -> String;
    /// Seconds since 1970, for rate limits.
    fn epoch_s(&self) -> u64;
}

pub trait Ids: Send + Sync {
    /// A new id for a song, a playlist or a job.
    fn id(&self) -> String;
    /// A private live key: 32 random URL-safe characters (SPEC 9).
    fn key(&self) -> String;
    /// A random seed for a request that brings none: 0 to 2³² − 1.
    fn seed(&self) -> i64;
}

/// The body of `POST /jobs` (SPEC 6).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EngineRequest {
    pub prompt: String,
    pub seconds: u32,
    pub bpm: Option<u32>,
    pub vocals: bool,
    pub lyrics: Option<String>,
    pub seed: i64,
    pub vocal_language: String,
    /// Asks ACE-Step to write the lyrics itself (SPEC 5, the lyrics fallback).
    pub write_lyrics: bool,
}

impl From<&Normalized> for EngineRequest {
    fn from(n: &Normalized) -> Self {
        Self {
            prompt: n.engine_prompt.clone(),
            seconds: n.seconds,
            bpm: n.bpm,
            vocals: n.vocals,
            lyrics: n.lyrics.clone(),
            seed: n.seed,
            vocal_language: n.vocal_language.clone(),
            write_lyrics: false,
        }
    }
}

/// `GET /jobs/{id}` (SPEC 6). `state` is `queued`, `running`, `done`, `failed` or `cancelled`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct RemoteJob {
    pub job_id: String,
    pub state: String,
    #[serde(default)]
    pub progress: f32,
    pub seconds: Option<f64>,
    pub seed: Option<i64>,
    pub error: Option<RemoteError>,
    pub lyrics: Option<String>,
    pub metas: Option<Metas>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RemoteError {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub retryable: bool,
}

/// What an engine learned while making a song.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Metas {
    pub bpm: Option<u32>,
    pub key: Option<String>,
    pub time_signature: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CallError {
    /// `429`: every slot is busy.
    Busy,
    /// `404`: the engine has no such job.
    NotFound,
    /// The engine refused the request (`422` and other client errors).
    Rejected(String),
    /// No answer, or one that made no sense.
    Transport(String),
}

impl fmt::Display for CallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy => f.write_str("the engine is busy"),
            Self::NotFound => f.write_str("the engine has no such job"),
            Self::Rejected(m) => write!(f, "the engine refused: {m}"),
            Self::Transport(m) => f.write_str(m),
        }
    }
}

/// One music engine server (SPEC 6). Health checks run inside the adapter; `status` reports the latest.
#[async_trait]
pub trait MusicEngine: Send + Sync {
    fn status(&self) -> EngineStatus;
    async fn submit(&self, request: &EngineRequest) -> std::result::Result<String, CallError>;
    async fn job(&self, id: &str) -> std::result::Result<RemoteJob, CallError>;
    /// Writes the finished job's WAV to `to`.
    async fn download(&self, id: &str, to: &Path) -> std::result::Result<(), CallError>;
    /// Cancels a running job or frees a finished one; failures are ignored.
    async fn delete(&self, id: &str);
}

#[derive(Debug, Clone, PartialEq)]
pub struct SongName {
    pub title: String,
    pub description: String,
}

/// A text engine (SPEC 7): writes lyrics and names songs, never music.
#[async_trait]
pub trait TextEngine: Send + Sync {
    fn id(&self) -> &str;
    /// False once a health check failed; true before the first check.
    fn healthy(&self) -> bool;
    async fn write_lyrics(&self, n: &Normalized) -> Result<String>;
    async fn name_song(&self, n: &Normalized, lyrics: Option<&str>) -> Result<SongName>;
}

/// Changes browsers hear about as events (SPEC 10, `/events`).
pub trait Notifier: Send + Sync {
    fn job(&self, job: &Job);
    fn song(&self, song: &Song);
}

#[cfg(test)]
mod tests {
    use domain::testing::normalized;

    use super::*;

    #[test]
    fn an_engine_request_carries_the_engine_prompt_not_the_persons_words() {
        let n = Normalized { prompt: Some("warm pads".into()), engine_prompt: "warm pads, calm mood, instrumental".into(), ..normalized() };
        let r = EngineRequest::from(&n);
        assert_eq!((r.prompt.as_str(), r.seconds, r.seed, r.write_lyrics), ("warm pads, calm mood, instrumental", 180, 7, false));
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!((v["vocal_language"].as_str(), v["write_lyrics"].as_bool()), (Some("en"), Some(false)));
    }

    #[test]
    fn a_remote_job_parses_with_optional_fields_missing() {
        let j: RemoteJob = serde_json::from_str(r#"{"job_id":"j","state":"running","progress":0.4}"#).unwrap();
        assert_eq!((j.state.as_str(), j.progress, j.metas), ("running", 0.4, None));
        let j: RemoteJob = serde_json::from_str(r#"{"job_id":"j","state":"failed","error":{"code":"oom","message":"m"}}"#).unwrap();
        assert!(!j.error.unwrap().retryable, "retryable defaults to false");
    }
}
