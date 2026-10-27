//! Asking for a song (arc42 8.2, "Making a song"): the request, its rules and the engine prompt.

use serde::{Deserialize, Serialize};

use crate::domain::Error;

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

/// `"auto"`: the one word some settings take in place of a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Auto {
    Auto,
}

/// `true`, `false`, or `"auto"`: Maio picks instrumental or vocals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Vocals {
    Set(bool),
    Auto(Auto),
}

/// Who sings: a hint added to the engine prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Voice {
    Female,
    Male,
    Duet,
}

impl Voice {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Female => "female",
            Self::Male => "male",
            Self::Duet => "duet",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [Self::Female, Self::Male, Self::Duet].into_iter().find(|v| v.as_str() == s)
    }

    fn words(self) -> &'static str {
        match self {
            Self::Female => "female vocals",
            Self::Male => "male vocals",
            Self::Duet => "male and female vocals",
        }
    }
}

/// The text the engine gets: the person's words, then the settings in words. The Make form shows it for the
/// settings alone while the prompt is empty, from this same function.
pub fn engine_prompt(
    prompt: Option<&str>,
    genre: Option<&str>,
    mood: Option<&str>,
    energy: Option<Energy>,
    bpm: Option<u32>,
    vocals: bool,
    voice: Option<Voice>,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.extend(prompt.map(String::from));
    parts.extend(genre.map(String::from));
    parts.extend(mood.map(|m| format!("{m} mood")));
    parts.extend(energy.map(|e| format!("{} energy", e.as_str())));
    parts.extend(bpm.map(|b| format!("{b} bpm")));
    match (vocals, voice) {
        (false, _) => parts.push("instrumental".into()),
        (true, Some(v)) => parts.push(v.words().into()),
        (true, None) => {}
    }
    parts.join(", ")
}

/// The body of `POST /songs` (arc42 8.2, the request table).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SongRequest {
    pub prompt: Option<String>,
    pub mood: Option<String>,
    pub genre: Option<String>,
    pub energy: Option<Energy>,
    pub seconds: Option<u32>,
    pub tempo: Option<Tempo>,
    pub vocals: Option<Vocals>,
    pub voice: Option<Voice>,
    pub lyrics: Option<String>,
    pub vocal_language: Option<String>,
    pub lyrics_by: Option<String>,
    pub engine: Option<String>,
    pub seed: Option<i64>,
    pub allow_noncommercial: Option<bool>,
}

/// What a request is checked against: config (arc42 7.3) and the text engines that exist.
pub struct Rules<'a> {
    pub moods: &'a [String],
    pub genres: &'a [String],
    pub text_engines: &'a [String],
    pub default_language: &'a str,
    pub default_lyrics_by: &'a str,
    /// The lengths an Auto length is picked from, shortest and longest.
    pub auto_seconds: (u32, u32),
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
    pub voice: Option<Voice>,
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
    /// Checks the request against arc42 8.2 and builds the engine prompt. `seed` is used when the request brings none.
    /// An Auto length and Auto vocals are picked from the seed, so the same seed and settings pick the same.
    pub fn normalize(&self, rules: &Rules, seed: i64) -> Result<Normalized, Error> {
        let seed = self.seed.unwrap_or(seed);
        // ponytail: one multiply spreads the seed's bits; enough to pick two settings, not a random generator.
        let mixed = (seed as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
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
        let (shortest, longest) = rules.auto_seconds;
        let steps = u64::from((longest - shortest) / 15 + 1);
        let seconds = self.seconds.unwrap_or(shortest + ((mixed >> 32) % steps) as u32 * 15);
        if !(15..=600).contains(&seconds) {
            return Err(Error::invalid("bad_seconds", "seconds must be 15 to 600"));
        }
        let bpm = match self.tempo {
            None => None,
            Some(Tempo::Word(w)) => Some(w.bpm()),
            Some(Tempo::Bpm(b)) if (40..=220).contains(&b) => Some(b),
            Some(Tempo::Bpm(_)) => return Err(Error::invalid("bad_tempo", "a tempo is slow, medium, fast, or 40 to 220 bpm")),
        };
        let vocals = match self.vocals {
            None => false,
            Some(Vocals::Set(v)) => v,
            Some(Vocals::Auto(_)) => mixed >> 63 == 1,
        };
        if self.voice.is_some() && self.vocals != Some(Vocals::Set(true)) {
            return Err(Error::invalid("voice_without_vocals", "a voice needs vocals: true"));
        }
        let lyrics = self.lyrics.as_deref().map(str::trim).filter(|l| !l.is_empty()).map(String::from);
        if lyrics.is_some() && self.vocals != Some(Vocals::Set(true)) {
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

        Ok(Normalized {
            engine_prompt: engine_prompt(prompt.as_deref(), self.genre.as_deref(), self.mood.as_deref(), self.energy, bpm, vocals, self.voice),
            prompt,
            mood: self.mood.clone(),
            genre: self.genre.clone(),
            energy: self.energy,
            seconds,
            bpm,
            vocals,
            voice: self.voice,
            lyrics,
            vocal_language,
            lyrics_by,
            engine: self.engine.clone(),
            seed,
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
        r.normalize(
            &Rules {
                moods: &moods,
                genres: &genres,
                text_engines: &text,
                default_language: "en",
                default_lyrics_by: "ace-step",
                auto_seconds: (120, 240),
            },
            99,
        )
    }

    fn rules() -> Rules<'static> {
        static MOODS: std::sync::LazyLock<Vec<String>> = std::sync::LazyLock::new(|| vec!["calm".into()]);
        Rules { moods: &MOODS, genres: &[], text_engines: &[], default_language: "en", default_lyrics_by: "ace-step", auto_seconds: (120, 240) }
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
        let n = run(&SongRequest { seconds: Some(180), ..r }).unwrap();
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
        assert_eq!(code(&SongRequest { vocals: Some(Vocals::Auto(Auto::Auto)), lyrics: Some("la".into()), ..calm() }), "lyrics_without_vocals");
        let n = run(&SongRequest { vocals: Some(Vocals::Set(true)), lyrics: Some("[chorus]\nla".into()), ..calm() }).unwrap();
        assert!(n.vocals && !n.engine_prompt.contains("instrumental"));
        assert_eq!(n.lyrics.as_deref(), Some("[chorus]\nla"));
        let blank = run(&SongRequest { vocals: Some(Vocals::Set(true)), lyrics: Some("  ".into()), ..calm() }).unwrap();
        assert_eq!(blank.lyrics, None, "blank lyrics are no lyrics");
        assert_eq!(code(&SongRequest { vocals: Some(Vocals::Set(true)), lyrics: Some("é".repeat(4001)), ..calm() }), "bad_lyrics");
    }

    #[test]
    fn language_and_lyrics_writer_default_from_config() {
        let sing = SongRequest { vocals: Some(Vocals::Set(true)), ..calm() };
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
    fn a_voice_is_a_hint_in_the_engine_prompt_and_needs_vocals() {
        let sing = |voice| run(&SongRequest { vocals: Some(Vocals::Set(true)), voice: Some(voice), ..calm() }).unwrap();
        assert_eq!(sing(Voice::Female).engine_prompt, "calm mood, female vocals");
        assert_eq!(sing(Voice::Male).engine_prompt, "calm mood, male vocals");
        assert_eq!(sing(Voice::Duet).engine_prompt, "calm mood, male and female vocals");
        assert_eq!(sing(Voice::Duet).voice, Some(Voice::Duet));
        assert_eq!(run(&SongRequest { vocals: Some(Vocals::Set(true)), ..calm() }).unwrap().engine_prompt, "calm mood", "vocals, any voice");
        assert_eq!(code(&SongRequest { voice: Some(Voice::Male), ..calm() }), "voice_without_vocals");
    }

    #[test]
    fn auto_length_and_auto_vocals_are_picked_from_the_seed() {
        let auto = SongRequest { vocals: Some(Vocals::Auto(Auto::Auto)), ..calm() };
        let picks: Vec<(u32, bool)> = (0..64).map(|seed| auto.normalize(&rules(), seed).map(|n| (n.seconds, n.vocals)).unwrap()).collect();
        assert!(picks.iter().all(|(s, _)| (120..=240).contains(s) && s % 15 == 0), "in the configured range, on a 15-second step: {picks:?}");
        assert!(picks.iter().any(|p| p.1) && picks.iter().any(|p| !p.1), "both instrumental and vocals come up");
        assert!(picks.iter().map(|p| p.0).collect::<std::collections::HashSet<_>>().len() > 3, "lengths vary");
        assert_eq!(auto.normalize(&rules(), 7), auto.normalize(&rules(), 7), "the same seed picks the same");
        assert_eq!(auto.normalize(&rules(), -5).unwrap().seed, -5, "any seed, negative too");
        assert!(!run(&calm()).unwrap().vocals, "nothing sent: instrumental, as before");
        assert_eq!(run(&SongRequest { seconds: Some(60), ..calm() }).unwrap().seconds, 60, "a length sent is kept");
    }

    #[test]
    fn the_engine_prompt_is_the_words_then_the_settings() {
        let p = engine_prompt(Some("warm pads"), Some("ambient"), Some("calm"), Some(Energy::Low), Some(96), true, Some(Voice::Female));
        assert_eq!(p, "warm pads, ambient, calm mood, low energy, 96 bpm, female vocals");
        assert_eq!(engine_prompt(None, None, Some("calm"), None, None, false, None), "calm mood, instrumental");
    }

    #[test]
    fn requests_parse_from_json_and_refuse_unknown_fields() {
        let r: SongRequest = serde_json::from_str(r#"{"mood":"calm","tempo":"slow","energy":"low","seconds":60}"#).unwrap();
        assert_eq!((r.tempo, r.energy, r.seconds), (Some(Tempo::Word(TempoWord::Slow)), Some(Energy::Low), Some(60)));
        assert_eq!(serde_json::from_str::<Tempo>("120").unwrap(), Tempo::Bpm(120));
        assert!(serde_json::from_str::<SongRequest>(r#"{"mood":"calm","bogus":1}"#).is_err());
        let r: SongRequest = serde_json::from_str(r#"{"mood":"calm","vocals":"auto","voice":"duet"}"#).unwrap();
        assert_eq!((r.vocals, r.voice), (Some(Vocals::Auto(Auto::Auto)), Some(Voice::Duet)));
        assert_eq!(serde_json::from_str::<SongRequest>(r#"{"vocals":true}"#).unwrap().vocals, Some(Vocals::Set(true)));
        assert_eq!(serde_json::to_string(&Vocals::Auto(Auto::Auto)).unwrap(), r#""auto""#);
    }
}
