//! Maio's core: songs, playlists and their rules (SPEC 1, 3). No I/O, no async.

mod error;
mod playlist;
mod song;
#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use error::{Error, PlaylistRef};
pub use playlist::{Live, MAX_NAME, Playlist, Visibility};
pub use song::{Commercial, Licence, MAX_TITLE, Song, TitleSource, song_deletable};

pub type SongId = String;
pub type PlaylistId = String;
