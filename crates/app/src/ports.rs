//! The ports: small traits the use cases need and adapters implement (SPEC 3).

use anyhow::Result;
use domain::{Playlist, PlaylistRef, Song};

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
    /// Rewrites the file's tags from the song, without re-encoding (SPEC 5).
    fn retag(&self, song: &Song) -> Result<()>;
    /// `file` is relative to the data folder, as `Song::file`.
    fn delete(&self, file: &str) -> Result<()>;
}

pub trait Clock: Send + Sync {
    /// UTC, `YYYY-MM-DDTHH:MM:SSZ`.
    fn now(&self) -> String;
}

pub trait Ids: Send + Sync {
    /// A new id for a song, a playlist or a job.
    fn id(&self) -> String;
    /// A private live key: 32 random URL-safe characters (SPEC 9).
    fn key(&self) -> String;
}
