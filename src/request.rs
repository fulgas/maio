use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{config::Scope, error::ApiError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
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
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Slow => "slow",
            Self::Medium => "medium",
            Self::Fast => "fast",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Tempo {
    Word(TempoWord),
    Bpm(u32),
}

/// The request object (SPEC 3). Every way of asking for music uses it.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub mood: Option<String>,
    pub genre: Option<String>,
    pub energy: Option<Energy>,
    pub prompt: Option<String>,
    pub seconds: Option<u32>,
    pub tempo: Option<Tempo>,
    pub vocals: Option<bool>,
    pub lyrics: Option<String>,
    pub engine: Option<String>,
    pub seed: Option<i64>,
    pub allow_noncommercial: Option<bool>,
    pub vocal_language: Option<String>,
    pub lyrics_by: Option<String>,
}

/// A validated request, ready for an engine.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Normalized {
    pub prompt: String,
    pub seconds: u32,
    pub bpm: Option<u32>,
    pub vocals: bool,
    pub lyrics: Option<String>,
    pub seed: i64,
    pub engine: Option<String>,
    pub allow_noncommercial: bool,
    pub mood: Option<String>,
    pub genre: Option<String>,
    pub energy: Option<Energy>,
    pub title: String,
    pub vocal_language: String,
    /// "ace-step" or a text engine id; `None` falls back to the playlist's, then config.
    pub lyrics_by: Option<String>,
    /// The prompt as a person wrote it; the only free text a text engine ever sees.
    pub user_prompt: Option<String>,
}

pub struct Rules<'a> {
    pub moods: &'a [String],
    pub genres: &'a [String],
    pub scopes: &'a HashSet<Scope>,
    pub playlist_name: Option<&'a str>,
    pub text_engines: &'a [String],
    pub default_language: &'a str,
}

fn not_allowed(message: impl Into<String>) -> ApiError {
    ApiError::forbidden("field_not_allowed", message)
}

pub fn normalize(r: &Request, rules: &Rules) -> Result<Normalized, ApiError> {
    let has = |s: Scope| rules.scopes.contains(&s);
    if r.mood.is_none() && r.prompt.is_none() {
        return Err(ApiError::invalid("missing_mood_or_prompt", "send mood, prompt, or both"));
    }
    if let Some(m) = &r.mood {
        if !rules.moods.contains(m) {
            return Err(ApiError::invalid("unknown_mood", format!("unknown mood {m:?}")));
        }
    }
    if let Some(g) = &r.genre {
        if !rules.genres.contains(g) {
            return Err(ApiError::invalid("unknown_genre", format!("unknown genre {g:?}")));
        }
    }
    if let Some(p) = &r.prompt {
        if !has(Scope::Queue) {
            return Err(not_allowed("prompt needs scope queue"));
        }
        if p.trim().is_empty() || p.chars().count() > 500 {
            return Err(ApiError::invalid("bad_prompt", "prompt must be 1 to 500 characters"));
        }
    }
    let seconds = r.seconds.unwrap_or(180);
    if !(15..=600).contains(&seconds) {
        return Err(ApiError::invalid("bad_seconds", "seconds must be 15 to 600"));
    }
    let bpm = match r.tempo {
        None => None,
        Some(Tempo::Word(w)) => Some(w.bpm()),
        Some(Tempo::Bpm(b)) if (40..=220).contains(&b) => Some(b),
        Some(Tempo::Bpm(_)) => return Err(ApiError::invalid("bad_tempo", "bpm must be 40 to 220")),
    };
    let vocals = r.vocals.unwrap_or(false);
    if vocals && !has(Scope::Queue) {
        return Err(not_allowed("vocals need scope queue"));
    }
    if r.lyrics.is_some() && !vocals {
        return Err(ApiError::invalid("lyrics_without_vocals", "lyrics need vocals: true"));
    }
    if r.engine.is_some() && !has(Scope::Queue) {
        return Err(not_allowed("engine needs scope queue"));
    }
    let allow_noncommercial = r.allow_noncommercial.unwrap_or(false);
    if allow_noncommercial && !has(Scope::Admin) {
        return Err(not_allowed("allow_noncommercial needs scope admin"));
    }

    let vocal_language = r.vocal_language.clone().unwrap_or_else(|| rules.default_language.to_string());
    if !crate::config::valid_language(&vocal_language) {
        return Err(ApiError::invalid("bad_language", "vocal_language is a language code such as \"en\""));
    }
    if let Some(by) = &r.lyrics_by {
        if !has(Scope::Queue) {
            return Err(not_allowed("lyrics_by needs scope queue"));
        }
        if by != "ace-step" && !rules.text_engines.contains(by) {
            return Err(ApiError::invalid("unknown_lyrics_engine", format!("no text engine {by:?}")));
        }
    }

    let mut parts: Vec<String> = Vec::new();
    if let Some(p) = &r.prompt {
        parts.push(p.trim().to_string());
    }
    if let Some(g) = &r.genre {
        parts.push(g.clone());
    }
    if let Some(m) = &r.mood {
        parts.push(format!("{m} mood"));
    }
    if let Some(e) = r.energy {
        parts.push(format!("{} energy", e.as_str()));
    }
    if let Some(b) = bpm {
        parts.push(format!("{b} bpm"));
    }
    if !vocals {
        parts.push("instrumental".into());
    }

    Ok(Normalized {
        prompt: parts.join(", "),
        seconds,
        bpm,
        vocals,
        lyrics: r.lyrics.clone(),
        seed: r.seed.unwrap_or_else(|| rand::random::<u32>() as i64),
        engine: r.engine.clone(),
        allow_noncommercial,
        mood: r.mood.clone(),
        genre: r.genre.clone(),
        energy: r.energy,
        title: title(r, rules.playlist_name),
        vocal_language,
        lyrics_by: r.lyrics_by.clone(),
        user_prompt: r.prompt.as_ref().map(|p| p.trim().to_string()),
    })
}

/// Track titles come from structured fields only, never the raw prompt (SPEC 6).
pub fn title(r: &Request, playlist_name: Option<&str>) -> String {
    if let Some(name) = playlist_name {
        return name.to_string();
    }
    let base = [r.mood.as_deref(), r.genre.as_deref()].into_iter().flatten().collect::<Vec<_>>().join(" ");
    if base.is_empty() {
        return "untitled".into();
    }
    match r.tempo {
        Some(Tempo::Word(w)) => format!("{base}, {}", w.as_str()),
        Some(Tempo::Bpm(b)) => format!("{base}, {b} bpm"),
        None => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    fn moods() -> Vec<String> {
        vec!["calm".into(), "tense".into()]
    }
    fn genres() -> Vec<String> {
        vec!["lo-fi".into(), "synthwave".into()]
    }
    fn scopes(list: &[Scope]) -> HashSet<Scope> {
        list.iter().copied().collect()
    }
    fn run(r: &Request, scopes: &HashSet<Scope>) -> Result<Normalized, ApiError> {
        let (m, g) = (moods(), genres());
        normalize(r, &Rules { moods: &m, genres: &g, scopes, playlist_name: None, text_engines: &["local".to_string()], default_language: "en" })
    }
    fn enum_request() -> Request {
        Request {
            mood: Some("calm".into()),
            genre: Some("lo-fi".into()),
            energy: Some(Energy::High),
            tempo: Some(Tempo::Word(TempoWord::Fast)),
            seconds: Some(180),
            ..Default::default()
        }
    }

    #[test]
    fn enum_request_builds_prompt_and_title() {
        let n = run(&enum_request(), &scopes(&[Scope::Read])).unwrap();
        assert_eq!(n.prompt, "lo-fi, calm mood, high energy, 135 bpm, instrumental");
        assert_eq!(n.title, "calm lo-fi, fast");
        assert_eq!((n.seconds, n.bpm, n.vocals), (180, Some(135), false));
    }

    #[test]
    fn prompt_needs_queue_scope_and_a_sane_length() {
        let r = Request { prompt: Some("warm pads".into()), ..Default::default() };
        assert_eq!(run(&r, &scopes(&[Scope::Read])).unwrap_err().code, "field_not_allowed");
        let n = run(&r, &scopes(&[Scope::Queue])).unwrap();
        assert_eq!(n.prompt, "warm pads, instrumental");
        assert_eq!(n.title, "untitled");
        assert_eq!(n.seconds, 180);
        let long = Request { prompt: Some("x".repeat(501)), ..Default::default() };
        assert_eq!(run(&long, &scopes(&[Scope::Queue])).unwrap_err().code, "bad_prompt");
    }

    #[test]
    fn rejects_unknown_mood_genre_and_bad_numbers() {
        let s = scopes(&[Scope::Queue]);
        let unknown = Request { mood: Some("angry".into()), ..Default::default() };
        assert_eq!(run(&unknown, &s).unwrap_err().code, "unknown_mood");
        let genre = Request { mood: Some("calm".into()), genre: Some("polka".into()), ..Default::default() };
        assert_eq!(run(&genre, &s).unwrap_err().code, "unknown_genre");
        let long = Request { mood: Some("calm".into()), seconds: Some(601), ..Default::default() };
        assert_eq!(run(&long, &s).unwrap_err().code, "bad_seconds");
        let bpm = Request { mood: Some("calm".into()), tempo: Some(Tempo::Bpm(300)), ..Default::default() };
        assert_eq!(run(&bpm, &s).unwrap_err().code, "bad_tempo");
        assert_eq!(run(&Request::default(), &s).unwrap_err().code, "missing_mood_or_prompt");
    }

    #[test]
    fn vocals_lyrics_and_noncommercial_rules() {
        let base = Request { mood: Some("calm".into()), ..Default::default() };
        let lyrics = Request { lyrics: Some("la".into()), ..base.clone() };
        assert_eq!(run(&lyrics, &scopes(&[Scope::Queue])).unwrap_err().code, "lyrics_without_vocals");
        let vocals = Request { vocals: Some(true), lyrics: Some("la".into()), ..base.clone() };
        let n = run(&vocals, &scopes(&[Scope::Queue])).unwrap();
        assert!(n.vocals && !n.prompt.contains("instrumental"));
        let nc = Request { allow_noncommercial: Some(true), ..base.clone() };
        assert_eq!(run(&nc, &scopes(&[Scope::Queue])).unwrap_err().code, "field_not_allowed");
        assert!(run(&nc, &scopes(&[Scope::Queue, Scope::Admin])).unwrap().allow_noncommercial);
    }

    #[test]
    fn seed_is_kept_or_random() {
        let s = scopes(&[Scope::Queue]);
        let r = Request { mood: Some("calm".into()), seed: Some(42), ..Default::default() };
        assert_eq!(run(&r, &s).unwrap().seed, 42);
        let a = run(&Request { seed: None, ..r.clone() }, &s).unwrap().seed;
        assert!(a >= 0);
    }

    #[test]
    fn titles() {
        let r = Request { mood: Some("tense".into()), genre: Some("synthwave".into()), tempo: Some(Tempo::Bpm(140)), ..Default::default() };
        assert_eq!(title(&r, None), "tense synthwave, 140 bpm");
        assert_eq!(title(&r, Some("late night coding")), "late night coding");
        assert_eq!(title(&Request { prompt: Some("x".into()), ..Default::default() }, None), "untitled");
    }

    #[test]
    fn tempo_parses_from_word_or_number() {
        assert_eq!(serde_json::from_str::<Tempo>("\"slow\"").unwrap(), Tempo::Word(TempoWord::Slow));
        assert_eq!(serde_json::from_str::<Tempo>("120").unwrap(), Tempo::Bpm(120));
        assert!(serde_json::from_str::<Request>(r#"{"mood":"calm","bogus":1}"#).is_err());
    }


    #[test]
    fn vocal_language_and_lyrics_by() {
        let q = scopes(&[Scope::Queue]);
        let sing = Request { mood: Some("calm".into()), vocals: Some(true), ..Default::default() };
        let n = run(&sing, &q).unwrap();
        assert_eq!((n.vocal_language.as_str(), n.lyrics_by.as_deref()), ("en", None), "default language from config");
        let n = run(&Request { vocal_language: Some("pt".into()), lyrics_by: Some("local".into()), ..sing.clone() }, &q).unwrap();
        assert_eq!((n.vocal_language.as_str(), n.lyrics_by.as_deref()), ("pt", Some("local")));
        assert!(run(&Request { lyrics_by: Some("ace-step".into()), ..sing.clone() }, &q).is_ok());
        assert_eq!(run(&Request { lyrics_by: Some("nope".into()), ..sing.clone() }, &q).unwrap_err().code, "unknown_lyrics_engine");
        assert_eq!(run(&Request { vocal_language: Some("English".into()), ..sing.clone() }, &q).unwrap_err().code, "bad_language");
        let read = scopes(&[Scope::Read]);
        assert_eq!(run(&Request { mood: Some("calm".into()), lyrics_by: Some("local".into()), ..Default::default() }, &read).unwrap_err().status, StatusCode::FORBIDDEN);
        assert_eq!(run(&Request { prompt: Some(" rainy bus ".into()), ..Default::default() }, &q).unwrap().user_prompt.as_deref(), Some("rainy bus"));
    }
}
