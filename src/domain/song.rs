//! A song and what it carries (arc42 8.2, "What a song carries").

use serde::{Deserialize, Serialize};

use crate::domain::{Error, PlaylistRef, SongId};

pub const MAX_TITLE: usize = 60;

/// Which layer gave a song its title (arc42 8.2, titles in layers).
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

/// An engine licence's commercial terms (arc42 3.3, `GET /info`).
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
    /// A person's title, never overwritten by the other layers (arc42 8.2, layer 1).
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

/// A song can be deleted only when no playlist holds it (arc42 1.2).
pub fn song_deletable(in_playlists: Vec<PlaylistRef>) -> Result<(), Error> {
    if in_playlists.is_empty() { Ok(()) } else { Err(Error::SongInPlaylist(in_playlists)) }
}

/// A format a song downloads in (arc42 3.5, `GET /songs/{id}/download`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadFormat {
    Mp3,
    Aac,
    Flac,
    Wav,
    Opus,
}

impl DownloadFormat {
    pub const ALL: [Self; 5] = [Self::Mp3, Self::Aac, Self::Flac, Self::Wav, Self::Opus];

    pub fn parse(s: &str) -> Result<Self, Error> {
        Self::ALL
            .into_iter()
            .find(|f| f.label().eq_ignore_ascii_case(s))
            .ok_or_else(|| Error::invalid("bad_format", "format is mp3, aac, flac, wav or opus"))
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Mp3 => "MP3",
            Self::Aac => "AAC",
            Self::Flac => "FLAC",
            Self::Wav => "WAV",
            Self::Opus => "Opus",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Mp3 => "mp3",
            Self::Aac => "m4a",
            Self::Flac => "flac",
            Self::Wav => "wav",
            Self::Opus => "opus",
        }
    }

    pub fn content_type(self) -> &'static str {
        match self {
            Self::Mp3 => "audio/mpeg",
            Self::Aac => "audio/mp4",
            Self::Flac => "audio/flac",
            Self::Wav => "audio/wav",
            Self::Opus => "audio/ogg",
        }
    }
}

/// A download's file name: the title without characters file systems refuse, at most 100 characters.
pub fn download_name(title: &str, format: DownloadFormat) -> String {
    let kept: String = title.chars().filter(|c| !c.is_control() && !r#"/\:*?"<>|"#.contains(*c)).collect();
    let words = kept.split_whitespace().collect::<Vec<_>>().join(" ");
    let base: String = words.trim_matches(|c: char| c == '.' || c.is_whitespace()).chars().take(100).collect();
    format!("{}.{}", if base.is_empty() { "song" } else { &base }, format.extension())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Error, PlaylistRef, testing::song};

    #[test]
    fn download_formats_parse_and_name_their_files() {
        assert_eq!(DownloadFormat::parse("mp3"), Ok(DownloadFormat::Mp3));
        assert_eq!(DownloadFormat::parse("FLAC"), Ok(DownloadFormat::Flac));
        assert_eq!(DownloadFormat::parse("ogg").unwrap_err().code(), "bad_format");
        assert_eq!(DownloadFormat::Aac.extension(), "m4a");
        assert_eq!(DownloadFormat::Opus.content_type(), "audio/ogg");
        assert_eq!(DownloadFormat::ALL.len(), 5);
    }

    #[test]
    fn download_names_are_safe() {
        assert_eq!(download_name("Soft Static", DownloadFormat::Mp3), "Soft Static.mp3");
        assert_eq!(download_name("AC/DC: Live?  \"Now\"", DownloadFormat::Wav), "ACDC Live Now.wav");
        assert_eq!(download_name("東京の夜", DownloadFormat::Flac), "東京の夜.flac");
        assert_eq!(download_name(" ..// ", DownloadFormat::Opus), "song.opus");
        assert!(download_name(&"x".repeat(300), DownloadFormat::Mp3).chars().count() <= 104);
    }

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
