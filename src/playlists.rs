//! Playlists (SPEC 10): everything that can feed a radio is a playlist.

use std::collections::HashSet;

use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::{
    config::Scope,
    error::ApiError,
    request::{Energy, Request, Rules, Tempo, normalize},
    store::Store,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Crate,
    Kept,
    Requests,
    Fixed,
    Generative,
    Smart,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Crate => "crate",
            Self::Kept => "kept",
            Self::Requests => "requests",
            Self::Fixed => "fixed",
            Self::Generative => "generative",
            Self::Smart => "smart",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        [Self::Crate, Self::Kept, Self::Requests, Self::Fixed, Self::Generative, Self::Smart].into_iter().find(|k| k.as_str() == s)
    }
    /// Crate, Kept and Requests: one each per install, never edited or deleted.
    pub fn builtin(self) -> bool {
        matches!(self, Self::Crate | Self::Kept | Self::Requests)
    }
    /// Kinds whose tracks are stored, so people add, remove and reorder them.
    pub fn stored(self) -> bool {
        matches!(self, Self::Fixed | Self::Generative)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Order {
    InOrder,
    Shuffle,
}

impl Order {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InOrder => "in_order",
            Self::Shuffle => "shuffle",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        [Self::InOrder, Self::Shuffle].into_iter().find(|o| o.as_str() == s)
    }
}

/// How a generative playlist makes tracks: the stage 1 station fields (SPEC 3, Stations; SPEC 10).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub prompt: Option<String>,
    pub mood: Option<String>,
    pub genre: Option<String>,
    pub energy: Option<Energy>,
    pub seconds: [u32; 2],
    pub tempo: Option<Tempo>,
    #[serde(default)]
    pub variations: Vec<String>,
    #[serde(default = "default_share")]
    pub library_share: f32,
    pub engine: Option<String>,
    /// Generative playlists choose (SPEC 10, Lyrics); instrumental stays the default.
    #[serde(default)]
    pub vocals: bool,
    pub vocal_language: Option<String>,
    pub lyrics_by: Option<String>,
}

fn default_share() -> f32 {
    0.25
}

impl Recipe {
    pub fn validate(&self, name: &str, cfg: &crate::config::Config) -> Result<(), ApiError> {
        let [lo, hi] = self.seconds;
        if !(15..=600).contains(&lo) || !(15..=600).contains(&hi) || lo > hi {
            return Err(ApiError::invalid("bad_seconds", "seconds must be [min, max], each 15 to 600"));
        }
        if !(0.0..=1.0).contains(&self.library_share) {
            return Err(ApiError::invalid("bad_library_share", "library_share must be 0 to 1"));
        }
        let scopes: HashSet<Scope> = [Scope::Queue].into();
        let ids = cfg.text_engine_ids();
        let rules = Rules { moods: &cfg.moods, genres: &cfg.genres, scopes: &scopes, playlist_name: Some(name),
                            text_engines: &ids, default_language: &cfg.lyrics.default_language };
        let variations = self.variations.iter().map(|v| Some(v.as_str())).chain([None]);
        for v in variations {
            normalize(&self.request_with(v, lo), &rules)?;
        }
        Ok(())
    }

    fn request_with(&self, variation: Option<&str>, seconds: u32) -> Request {
        let prompt = match (&self.prompt, variation) {
            (Some(p), Some(v)) => Some(format!("{p}, {v}")),
            (Some(p), None) => Some(p.clone()),
            (None, Some(v)) => Some(v.to_string()),
            (None, None) => None,
        };
        Request {
            mood: self.mood.clone(),
            genre: self.genre.clone(),
            energy: self.energy,
            prompt,
            seconds: Some(seconds),
            tempo: self.tempo,
            engine: self.engine.clone(),
            vocals: Some(self.vocals),
            vocal_language: self.vocal_language.clone(),
            lyrics_by: self.lyrics_by.clone(),
            ..Default::default()
        }
    }

    /// The next track: random length in range, one random variation, random seed.
    pub fn next_request(&self) -> Request {
        let mut rng = rand::rng();
        let seconds = rng.random_range(self.seconds[0]..=self.seconds[1]);
        let variation = (!self.variations.is_empty()).then(|| self.variations[rng.random_range(0..self.variations.len())].as_str());
        self.request_with(variation, seconds)
    }
}

/// Which crate tracks a smart playlist plays; every given field must match (SPEC 10).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mood: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kept: Option<bool>,
    /// `a`: asked for once by a person; `b`: made by a generative playlist.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_playlist: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_seconds: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub made_within_days: Option<u32>,
}

impl Rule {
    /// Field checks; whether `from_playlist` exists is checked by the caller, which has the store.
    pub fn validate(&self, moods: &[String], genres: &[String]) -> Result<(), ApiError> {
        if let Some(m) = &self.mood {
            if !moods.contains(m) {
                return Err(ApiError::invalid("unknown_mood", format!("unknown mood {m:?}")));
            }
        }
        if let Some(g) = &self.genre {
            if !genres.contains(g) {
                return Err(ApiError::invalid("unknown_genre", format!("unknown genre {g:?}")));
            }
        }
        if self.side.as_deref().is_some_and(|s| s != "a" && s != "b") {
            return Err(ApiError::invalid("bad_side", "side is a or b"));
        }
        if self.max_seconds.is_some_and(|s| !(15..=600).contains(&s)) {
            return Err(ApiError::invalid("bad_rule", "max_seconds must be 15 to 600"));
        }
        if self.made_within_days.is_some_and(|d| !(1..=3650).contains(&d)) {
            return Err(ApiError::invalid("bad_rule", "made_within_days must be 1 to 3650"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    pub order: Order,
    /// `None` follows `[playlists] max_tracks`; shown through `limit`.
    #[serde(skip)]
    pub max_tracks: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipe: Option<Recipe>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<Rule>,
    pub created_at: String,
}

/// Built-in ids, in the order lists show them.
pub const BUILTINS: [&str; 3] = ["crate", "kept", "requests"];

impl Playlist {
    pub fn builtin(id: &str) -> Option<Self> {
        let (name, kind) = match id {
            "crate" => ("Crate", Kind::Crate),
            "kept" => ("Kept", Kind::Kept),
            "requests" => ("Requests", Kind::Requests),
            _ => return None,
        };
        Some(Self { id: id.into(), name: name.into(), kind, order: Order::Shuffle, max_tracks: None, recipe: None, rule: None, created_at: String::new() })
    }

    /// The size limit in force; built-ins have none.
    pub fn limit(&self, default: u32) -> Option<u32> {
        (!self.kind.builtin()).then(|| self.max_tracks.unwrap_or(default))
    }

    /// How many matches a smart playlist plays; other kinds are not capped when read.
    pub fn cap(&self, default: u32) -> Option<u32> {
        (self.kind == Kind::Smart).then(|| self.max_tracks.unwrap_or(default))
    }
}

/// A built-in or a stored playlist.
pub fn find(store: &Store, id: &str) -> rusqlite::Result<Option<Playlist>> {
    match Playlist::builtin(id) {
        Some(p) => Ok(Some(p)),
        None => store.playlist(id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_and_orders_round_trip_and_builtins_have_no_limit() {
        for k in ["crate", "kept", "requests", "fixed", "generative", "smart"] {
            assert_eq!(Kind::parse(k).unwrap().as_str(), k);
        }
        assert_eq!(Order::parse("in_order"), Some(Order::InOrder));
        assert!(Kind::parse("station").is_none());
        let crate_ = Playlist::builtin("crate").unwrap();
        assert_eq!((crate_.kind, crate_.limit(50), crate_.cap(50)), (Kind::Crate, None, None));
        let smart = Playlist { kind: Kind::Smart, max_tracks: Some(7), ..crate_.clone() };
        assert_eq!((smart.limit(50), smart.cap(50)), (Some(7), Some(7)));
        let fixed = Playlist { kind: Kind::Fixed, ..crate_ };
        assert_eq!((fixed.limit(50), fixed.cap(50)), (Some(50), None));
    }

    #[test]
    fn rule_validation() {
        let (m, g) = (vec!["calm".to_string()], vec!["lo-fi".to_string()]);
        assert!(Rule { mood: Some("calm".into()), side: Some("b".into()), max_seconds: Some(200), ..Default::default() }.validate(&m, &g).is_ok());
        assert_eq!(Rule { mood: Some("angry".into()), ..Default::default() }.validate(&m, &g).unwrap_err().code, "unknown_mood");
        assert_eq!(Rule { side: Some("c".into()), ..Default::default() }.validate(&m, &g).unwrap_err().code, "bad_side");
        assert_eq!(Rule { made_within_days: Some(0), ..Default::default() }.validate(&m, &g).unwrap_err().code, "bad_rule");
        assert!(serde_json::from_str::<Rule>(r#"{"bogus":1}"#).is_err());
    }

    fn recipe() -> Recipe {
        Recipe {
            prompt: Some("lo-fi hip hop, warm vinyl".into()),
            mood: Some("calm".into()),
            genre: Some("lo-fi".into()),
            energy: None,
            seconds: [150, 240],
            tempo: Some(crate::request::Tempo::Word(crate::request::TempoWord::Slow)),
            variations: vec!["with rain ambience".into(), "jazzy chords".into()],
            library_share: 0.25,
            engine: None,
            vocals: false,
            vocal_language: None,
            lyrics_by: None,
        }
    }

    #[test]
    fn recipe_validation() {
        let cfg = crate::config::parse("data_dir = \"/tmp/x\"\nmoods = [\"calm\"]\ngenres = [\"lo-fi\"]\n", |_| None).unwrap();
        assert!(recipe().validate("late night", &cfg).is_ok());
        assert_eq!(Recipe { seconds: [240, 150], ..recipe() }.validate("x", &cfg).unwrap_err().code, "bad_seconds");
        assert_eq!(Recipe { library_share: 1.5, ..recipe() }.validate("x", &cfg).unwrap_err().code, "bad_library_share");
        assert_eq!(Recipe { mood: Some("angry".into()), ..recipe() }.validate("x", &cfg).unwrap_err().code, "unknown_mood");
        assert!(serde_json::from_str::<Recipe>(r#"{"name":"x","seconds":[20,30],"mood":"calm"}"#).is_err(), "name lives on the playlist");
        assert_eq!(Recipe { vocals: true, lyrics_by: Some("nope".into()), ..recipe() }.validate("x", &cfg).unwrap_err().code, "unknown_lyrics_engine");
    }

    #[test]
    fn recipe_next_request_varies_within_range() {
        let r = recipe();
        for _ in 0..50 {
            let q = r.next_request();
            assert!((150..=240).contains(&q.seconds.unwrap()));
            let p = q.prompt.unwrap();
            assert!(p == "lo-fi hip hop, warm vinyl, with rain ambience" || p == "lo-fi hip hop, warm vinyl, jazzy chords");
            assert_eq!(q.vocals, Some(false));
        }
    }
}
