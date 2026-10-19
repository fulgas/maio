//! A song and what it carries (SPEC 5, "What a song carries").

use serde::{Deserialize, Serialize};

use crate::{Error, PlaylistRef, SongId};

pub const MAX_TITLE: usize = 60;

/// Which layer gave a song its title (SPEC 5, titles in layers).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TitleSource {
    Person,
    Engine,
    Lyrics,
    Words,
}

impl TitleSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Engine => "engine",
            Self::Lyrics => "lyrics",
            Self::Words => "words",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [Self::Person, Self::Engine, Self::Lyrics, Self::Words].into_iter().find(|t| t.as_str() == s)
    }
}

/// An engine licence's commercial terms (SPEC 6, `GET /info`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

    pub fn parse(s: &str) -> Option<Self> {
        [Self::Yes, Self::Limited, Self::No].into_iter().find(|c| c.as_str() == s)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Licence {
    pub name: String,
    pub commercial: Commercial,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Song {
    pub id: SongId,
    pub title: String,
    pub title_source: TitleSource,
    pub description: Option<String>,
    /// What the person wrote; empty when they asked by mood only.
    pub prompt: String,
    pub mood: Option<String>,
    pub genre: Option<String>,
    pub energy: Option<String>,
    pub seconds: f64,
    pub bpm: Option<u32>,
    pub seed: i64,
    pub vocals: bool,
    pub key: Option<String>,
    pub time_signature: Option<String>,
    pub lyrics: Option<String>,
    pub vocal_language: Option<String>,
    pub engine: String,
    pub licence: Licence,
    pub lyrics_engine: Option<String>,
    pub title_engine: Option<String>,
    pub artist: String,
    /// Relative to the data folder: `songs/{id}.{ext}`.
    pub file: String,
    pub bytes: u64,
    pub created_at: String,
}

impl Song {
    /// A person's title, never overwritten by the other layers (SPEC 5, layer 1).
    pub fn rename(&mut self, title: &str) -> Result<(), Error> {
        let title = title.trim();
        let n = title.chars().count();
        if n == 0 || n > MAX_TITLE {
            return Err(Error::invalid("bad_title", format!("a title is 1 to {MAX_TITLE} characters")));
        }
        self.title = title.to_string();
        self.title_source = TitleSource::Person;
        Ok(())
    }
}

/// A song can be deleted only when no playlist holds it (SPEC 1).
pub fn song_deletable(in_playlists: Vec<PlaylistRef>) -> Result<(), Error> {
    if in_playlists.is_empty() { Ok(()) } else { Err(Error::SongInPlaylist(in_playlists)) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Error, PlaylistRef, testing::song};

    #[test]
    fn a_rename_is_a_person_title_trimmed() {
        let mut s = song("a");
        s.rename("  Night Bus  ").unwrap();
        assert_eq!((s.title.as_str(), s.title_source), ("Night Bus", TitleSource::Person));
    }

    #[test]
    fn a_rename_counts_characters_not_bytes() {
        let mut s = song("a");
        s.rename(&"é".repeat(60)).unwrap();
        assert_eq!(s.rename(&"é".repeat(61)).unwrap_err().code(), "bad_title");
        assert_eq!(s.rename("   ").unwrap_err().code(), "bad_title");
        assert_eq!(s.title, "é".repeat(60), "a refused rename changes nothing");
    }

    #[test]
    fn only_a_song_in_no_playlist_can_be_deleted() {
        assert_eq!(song_deletable(vec![]), Ok(()));
        let p = vec![PlaylistRef { id: "p1".into(), name: "Late".into() }];
        assert_eq!(song_deletable(p.clone()), Err(Error::SongInPlaylist(p)));
    }

    #[test]
    fn sources_and_licence_terms_round_trip_as_text() {
        for t in [TitleSource::Person, TitleSource::Engine, TitleSource::Lyrics, TitleSource::Words] {
            assert_eq!(TitleSource::parse(t.as_str()), Some(t));
        }
        for c in [Commercial::Yes, Commercial::Limited, Commercial::No] {
            assert_eq!(Commercial::parse(c.as_str()), Some(c));
        }
        assert_eq!(TitleSource::parse("crate"), None);
    }
}
