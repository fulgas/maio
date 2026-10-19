//! Asking for a song (SPEC 5, "Making a song"): the request, its rules and the engine prompt.

use serde::{Deserialize, Serialize};

use crate::Error;

pub const MAX_PROMPT: usize = 500;
pub const MAX_LYRICS: usize = 4000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Energy {
    Low,
    Medium,
    High,
}

impl Energy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TempoWord {
    Slow,
    Medium,
    Fast,
}

impl TempoWord {
    pub fn bpm(self) -> u32 {
        match self {
            Self::Slow => 75,
            Self::Medium => 105,
            Self::Fast => 135,
        }
    }
}

/// `"slow"`, `"medium"`, `"fast"`, or a BPM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Tempo {
    Word(TempoWord),
    Bpm(u32),
}

/// The body of `POST /songs` (SPEC 5, the request table).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SongRequest {
    pub prompt: Option<String>,
    pub mood: Option<String>,
    pub genre: Option<String>,
    pub energy: Option<Energy>,
    pub seconds: Option<u32>,
    pub tempo: Option<Tempo>,
    pub vocals: Option<bool>,
    pub lyrics: Option<String>,
    pub vocal_language: Option<String>,
    pub lyrics_by: Option<String>,
    pub engine: Option<String>,
    pub seed: Option<i64>,
    pub allow_noncommercial: Option<bool>,
}

/// What a request is checked against: config (SPEC 11) and the text engines that exist.
pub struct Rules<'a> {
    pub moods: &'a [String],
    pub genres: &'a [String],
    pub text_engines: &'a [String],
    pub default_language: &'a str,
    pub default_lyrics_by: &'a str,
}

/// A checked request, ready for an engine.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Normalized {
    /// What the engine gets: the person's words plus the structured fields.
    pub engine_prompt: String,
    /// The person's own words, trimmed: the only free text a text engine ever sees.
    pub prompt: Option<String>,
    pub mood: Option<String>,
    pub genre: Option<String>,
    pub energy: Option<Energy>,
    pub seconds: u32,
    pub bpm: Option<u32>,
    pub vocals: bool,
    pub lyrics: Option<String>,
    pub vocal_language: String,
    /// `"ace-step"` or a text engine id.
    pub lyrics_by: String,
    pub engine: Option<String>,
    pub seed: i64,
    pub allow_noncommercial: bool,
}

/// A language code such as `en` or `pt`.
pub fn valid_language(s: &str) -> bool {
    (2..=3).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_lowercase())
}

impl SongRequest {
    /// Checks the request against SPEC 5 and builds the engine prompt. `seed` is used when the request brings none.
    pub fn normalize(&self, rules: &Rules, seed: i64) -> Result<Normalized, Error> {
        let prompt = self.prompt.as_deref().map(|p| p.trim().to_string());
        if prompt.is_none() && self.mood.is_none() {
            return Err(Error::invalid("missing_mood_or_prompt", "send a prompt, a mood, or both"));
        }
        if prompt.as_ref().is_some_and(|p| p.is_empty() || p.chars().count() > MAX_PROMPT) {
            return Err(Error::invalid("bad_prompt", format!("a prompt is 1 to {MAX_PROMPT} characters")));
        }
        if let Some(m) = self.mood.as_ref().filter(|m| !rules.moods.contains(m)) {
            return Err(Error::invalid("unknown_mood", format!("unknown mood {m:?}")));
        }
        if let Some(g) = self.genre.as_ref().filter(|g| !rules.genres.contains(g)) {
            return Err(Error::invalid("unknown_genre", format!("unknown genre {g:?}")));
        }
        let seconds = self.seconds.unwrap_or(180);
        if !(15..=600).contains(&seconds) {
            return Err(Error::invalid("bad_seconds", "seconds must be 15 to 600"));
        }
        let bpm = match self.tempo {
            None => None,
            Some(Tempo::Word(w)) => Some(w.bpm()),
            Some(Tempo::Bpm(b)) if (40..=220).contains(&b) => Some(b),
            Some(Tempo::Bpm(_)) => return Err(Error::invalid("bad_tempo", "a tempo is slow, medium, fast, or 40 to 220 bpm")),
        };
        let vocals = self.vocals.unwrap_or(false);
        let lyrics = self.lyrics.as_deref().map(str::trim).filter(|l| !l.is_empty()).map(String::from);
        if lyrics.is_some() && !vocals {
            return Err(Error::invalid("lyrics_without_vocals", "lyrics need vocals: true"));
        }
        if lyrics.as_ref().is_some_and(|l| l.chars().count() > MAX_LYRICS) {
            return Err(Error::invalid("bad_lyrics", format!("lyrics are at most {MAX_LYRICS} characters")));
        }
        let vocal_language = self.vocal_language.clone().unwrap_or_else(|| rules.default_language.to_string());
        if !valid_language(&vocal_language) {
            return Err(Error::invalid("bad_language", "vocal_language is a language code such as \"en\""));
        }
        let lyrics_by = self.lyrics_by.clone().unwrap_or_else(|| rules.default_lyrics_by.to_string());
        if lyrics_by != "ace-step" && !rules.text_engines.contains(&lyrics_by) {
            return Err(Error::invalid("unknown_lyrics_engine", format!("no text engine {lyrics_by:?}")));
        }

        let mut parts: Vec<String> = Vec::new();
        parts.extend(prompt.clone());
        parts.extend(self.genre.clone());
        parts.extend(self.mood.as_ref().map(|m| format!("{m} mood")));
        parts.extend(self.energy.map(|e| format!("{} energy", e.as_str())));
        parts.extend(bpm.map(|b| format!("{b} bpm")));
        if !vocals {
            parts.push("instrumental".into());
        }

        Ok(Normalized {
            engine_prompt: parts.join(", "),
            prompt,
            mood: self.mood.clone(),
            genre: self.genre.clone(),
            energy: self.energy,
            seconds,
            bpm,
            vocals,
            lyrics,
            vocal_language,
            lyrics_by,
            engine: self.engine.clone(),
            seed: self.seed.unwrap_or(seed),
            allow_noncommercial: self.allow_noncommercial.unwrap_or(false),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(r: &SongRequest) -> Result<Normalized, Error> {
        let moods = vec!["calm".to_string(), "tense".into()];
        let genres = vec!["lo-fi".to_string(), "synthwave".into()];
        let text = vec!["local".to_string()];
        r.normalize(&Rules { moods: &moods, genres: &genres, text_engines: &text, default_language: "en", default_lyrics_by: "ace-step" }, 99)
    }

    fn code(r: &SongRequest) -> &'static str {
        run(r).unwrap_err().code()
    }

    fn calm() -> SongRequest {
        SongRequest { mood: Some("calm".into()), ..Default::default() }
    }

    #[test]
    fn structured_fields_build_the_engine_prompt() {
        let r = SongRequest {
            prompt: Some("  warm pads ".into()),
            genre: Some("lo-fi".into()),
            energy: Some(Energy::High),
            tempo: Some(Tempo::Word(TempoWord::Fast)),
            ..calm()
        };
        let n = run(&r).unwrap();
        assert_eq!(n.engine_prompt, "warm pads, lo-fi, calm mood, high energy, 135 bpm, instrumental");
        assert_eq!((n.prompt.as_deref(), n.seconds, n.bpm, n.vocals), (Some("warm pads"), 180, Some(135), false));
        assert!(!n.allow_noncommercial);
    }

    #[test]
    fn a_request_needs_a_prompt_or_a_mood_and_sane_values() {
        assert_eq!(code(&SongRequest::default()), "missing_mood_or_prompt");
        assert_eq!(code(&SongRequest { prompt: Some("x".repeat(501)), ..Default::default() }), "bad_prompt");
        assert!(run(&SongRequest { prompt: Some("é".repeat(500)), ..Default::default() }).is_ok(), "characters, not bytes");
        assert_eq!(code(&SongRequest { prompt: Some("   ".into()), ..Default::default() }), "bad_prompt");
        assert_eq!(code(&SongRequest { mood: Some("angry".into()), ..Default::default() }), "unknown_mood");
        assert_eq!(code(&SongRequest { genre: Some("polka".into()), ..calm() }), "unknown_genre");
        assert_eq!(code(&SongRequest { seconds: Some(14), ..calm() }), "bad_seconds");
        assert_eq!(code(&SongRequest { seconds: Some(601), ..calm() }), "bad_seconds");
        assert_eq!(code(&SongRequest { tempo: Some(Tempo::Bpm(221)), ..calm() }), "bad_tempo");
        assert_eq!(run(&SongRequest { tempo: Some(Tempo::Bpm(40)), ..calm() }).unwrap().bpm, Some(40));
    }

    #[test]
    fn vocals_are_off_unless_asked_and_lyrics_need_them() {
        assert_eq!(code(&SongRequest { lyrics: Some("la".into()), ..calm() }), "lyrics_without_vocals");
        let n = run(&SongRequest { vocals: Some(true), lyrics: Some("[chorus]\nla".into()), ..calm() }).unwrap();
        assert!(n.vocals && !n.engine_prompt.contains("instrumental"));
        assert_eq!(n.lyrics.as_deref(), Some("[chorus]\nla"));
        let blank = run(&SongRequest { vocals: Some(true), lyrics: Some("  ".into()), ..calm() }).unwrap();
        assert_eq!(blank.lyrics, None, "blank lyrics are no lyrics");
        assert_eq!(code(&SongRequest { vocals: Some(true), lyrics: Some("é".repeat(4001)), ..calm() }), "bad_lyrics");
    }

    #[test]
    fn language_and_lyrics_writer_default_from_config() {
        let sing = SongRequest { vocals: Some(true), ..calm() };
        let n = run(&sing).unwrap();
        assert_eq!((n.vocal_language.as_str(), n.lyrics_by.as_str()), ("en", "ace-step"));
        let n = run(&SongRequest { vocal_language: Some("pt".into()), lyrics_by: Some("local".into()), ..sing.clone() }).unwrap();
        assert_eq!((n.vocal_language.as_str(), n.lyrics_by.as_str()), ("pt", "local"));
        assert_eq!(code(&SongRequest { lyrics_by: Some("nope".into()), ..sing.clone() }), "unknown_lyrics_engine");
        assert_eq!(code(&SongRequest { vocal_language: Some("English".into()), ..sing }), "bad_language");
    }

    #[test]
    fn the_seed_is_kept_or_the_given_one_is_used() {
        assert_eq!(run(&SongRequest { seed: Some(42), ..calm() }).unwrap().seed, 42);
        assert_eq!(run(&calm()).unwrap().seed, 99);
    }

    #[test]
    fn requests_parse_from_json_and_refuse_unknown_fields() {
        let r: SongRequest = serde_json::from_str(r#"{"mood":"calm","tempo":"slow","energy":"low","seconds":60}"#).unwrap();
        assert_eq!((r.tempo, r.energy, r.seconds), (Some(Tempo::Word(TempoWord::Slow)), Some(Energy::Low), Some(60)));
        assert_eq!(serde_json::from_str::<Tempo>("120").unwrap(), Tempo::Bpm(120));
        assert!(serde_json::from_str::<SongRequest>(r#"{"mood":"calm","bogus":1}"#).is_err());
    }
}
