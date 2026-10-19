//! Maio's core: songs, playlists and their rules (SPEC 1, 3). No I/O, no async.

pub mod account;
pub mod engine;
mod error;
pub mod job;
pub mod limits;
pub mod live;
mod playlist;
pub mod request;
mod song;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
pub mod titles;

pub use error::{Error, PlaylistRef};
pub use playlist::{Live, LiveChange, MAX_NAME, Playlist, Visibility};
pub use song::{Commercial, DownloadFormat, Licence, MAX_TITLE, Song, TitleSource, download_name, song_deletable};

pub type SongId = String;
pub type PlaylistId = String;
