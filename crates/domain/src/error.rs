//! What a rule can refuse, each with the code the API sends (SPEC 10).

use std::fmt;

use serde::{Deserialize, Serialize};

/// A playlist as messages and lists name it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaylistRef {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    /// `"song"` or `"playlist"`.
    NotFound(&'static str),
    Invalid { code: &'static str, message: String },
    /// The playlists that still hold the song.
    SongInPlaylist(Vec<PlaylistRef>),
    PlaylistLive,
}

impl Error {
    pub fn invalid(code: &'static str, message: impl Into<String>) -> Self {
        Self::Invalid { code, message: message.into() }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound(_) => "not_found",
            Self::Invalid { code, .. } => code,
            Self::SongInPlaylist(_) => "song_in_playlist",
            Self::PlaylistLive => "playlist_live",
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(what) => write!(f, "no such {what}"),
            Self::Invalid { message, .. } => f.write_str(message),
            Self::SongInPlaylist(p) => write!(f, "the song is in {} playlist(s); remove it from them first", p.len()),
            Self::PlaylistLive => f.write_str("the playlist is live; take it off air first"),
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_error_has_the_spec_code() {
        assert_eq!(Error::NotFound("song").code(), "not_found");
        assert_eq!(Error::invalid("bad_title", "x").code(), "bad_title");
        assert_eq!(Error::SongInPlaylist(vec![]).code(), "song_in_playlist");
        assert_eq!(Error::PlaylistLive.code(), "playlist_live");
        assert_eq!(Error::NotFound("playlist").to_string(), "no such playlist");
    }
}
