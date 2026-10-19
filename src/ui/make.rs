//! The Make form (SPEC 5, 12): what the person picked, turned into a song request.

use crate::domain::request::{Energy, SongRequest, Tempo, TempoWord};

/// The length chip: (seconds, label).
pub const LENGTHS: [(u32, &str); 7] = [(30, "0:30"), (60, "1:00"), (120, "2:00"), (180, "3:00"), (240, "4:00"), (300, "5:00"), (600, "10:00")];
/// The tempo chip: (value, label); a value is a tempo word or a BPM.
pub const TEMPOS: [(&str, &str); 9] = [
    ("", "Any tempo"),
    ("slow", "Slow"),
    ("medium", "Medium"),
    ("fast", "Fast"),
    ("70", "70 bpm"),
    ("90", "90 bpm"),
    ("110", "110 bpm"),
    ("128", "128 bpm"),
    ("140", "140 bpm"),
];
/// "Sung in": (code, label); empty means the server's default.
pub const LANGUAGES: [(&str, &str); 10] = [
    ("", "Default language"),
    ("en", "English"),
    ("es", "Spanish"),
    ("fr", "French"),
    ("de", "German"),
    ("it", "Italian"),
    ("pt", "Portuguese"),
    ("ja", "Japanese"),
    ("ko", "Korean"),
    ("zh", "Chinese"),
];

#[derive(Debug, Clone, PartialEq)]
pub struct MakeForm {
    pub prompt: String,
    pub mood: String,
    pub genre: String,
    /// Not a chip: only "Make one like this" sets it, from a song made through the API.
    pub energy: String,
    pub seconds: u32,
    pub tempo: String,
    pub seed: String,
    pub vocals: bool,
    pub lyrics: String,
    pub language: String,
    pub lyrics_by: String,
}

impl Default for MakeForm {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            mood: String::new(),
            genre: String::new(),
            energy: String::new(),
            seconds: 180,
            tempo: String::new(),
            seed: String::new(),
            vocals: false,
            lyrics: String::new(),
            language: String::new(),
            lyrics_by: String::new(),
        }
    }
}

fn some(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

impl MakeForm {
    /// The request to send, or a sentence saying what to fix. The server checks everything again.
    pub fn request(&self, allow_noncommercial: bool) -> Result<SongRequest, String> {
        let (prompt, mood) = (some(&self.prompt), some(&self.mood));
        if prompt.is_none() && mood.is_none() {
            return Err("Describe the sound or pick a mood.".into());
        }
        let tempo = match self.tempo.trim() {
            "" => None,
            "slow" => Some(Tempo::Word(TempoWord::Slow)),
            "medium" => Some(Tempo::Word(TempoWord::Medium)),
            "fast" => Some(Tempo::Word(TempoWord::Fast)),
            bpm => match bpm.parse::<u32>() {
                Ok(b) if (40..=220).contains(&b) => Some(Tempo::Bpm(b)),
                _ => return Err("A tempo is 40 to 220 bpm.".into()),
            },
        };
        let seed = match self.seed.trim() {
            "" => None,
            s => Some(s.parse::<i64>().map_err(|_| "The seed is a whole number.".to_string())?),
        };
        let v = self.vocals;
        Ok(SongRequest {
            prompt,
            mood,
            genre: some(&self.genre),
            energy: match self.energy.as_str() {
                "low" => Some(Energy::Low),
                "medium" => Some(Energy::Medium),
                "high" => Some(Energy::High),
                _ => None,
            },
            seconds: Some(self.seconds),
            tempo,
            vocals: v.then_some(true),
            lyrics: if v { some(&self.lyrics) } else { None },
            vocal_language: if v { some(&self.language) } else { None },
            lyrics_by: if v { some(&self.lyrics_by) } else { None },
            engine: None,
            seed,
            allow_noncommercial: allow_noncommercial.then_some(true),
        })
    }
}

/// What "Make one like this" puts in the form (SPEC 12): the song's settings, its words and its seed.
impl MakeForm {
    pub fn from_song(s: &crate::domain::Song) -> Self {
        let nearest =
            LENGTHS.iter().min_by(|a, b| (f64::from(a.0) - s.seconds).abs().total_cmp(&(f64::from(b.0) - s.seconds).abs())).map_or(180, |l| l.0);
        Self {
            prompt: s.prompt.clone(),
            mood: s.mood.clone().unwrap_or_default(),
            genre: s.genre.clone().unwrap_or_default(),
            energy: s.energy.clone().unwrap_or_default(),
            seconds: nearest,
            tempo: s.bpm.filter(|b| (40..=220).contains(b)).map(|b| b.to_string()).unwrap_or_default(),
            seed: s.seed.to_string(),
            vocals: s.vocals,
            lyrics: if s.vocals { s.lyrics.clone().unwrap_or_default() } else { String::new() },
            language: if s.vocals { s.vocal_language.clone().unwrap_or_default() } else { String::new() },
            lyrics_by: String::new(),
        }
    }
}

/// The tempo chip's choices, plus the current bpm when it is not one of them.
pub fn tempo_options(current: &str) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = TEMPOS.iter().map(|(v, l)| (v.to_string(), l.to_string())).collect();
    if !current.is_empty() && !v.iter().any(|(x, _)| x == current) {
        v.push((current.to_string(), format!("{current} bpm")));
    }
    v
}

/// A form for the Make box to take up: "Make one like this" sets it, the Make box empties it.
#[derive(Clone, Copy)]
pub struct Draft(pub leptos::prelude::RwSignal<Option<MakeForm>>);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::request::{Tempo, TempoWord};

    fn form() -> MakeForm {
        MakeForm { prompt: "warm pads".into(), ..MakeForm::default() }
    }

    #[test]
    fn a_prompt_alone_makes_an_instrumental_three_minute_song() {
        let r = form().request(false).unwrap();
        assert_eq!(r.prompt.as_deref(), Some("warm pads"));
        assert_eq!(r.seconds, Some(180));
        assert_eq!(r.vocals, None);
        assert_eq!(r.lyrics, None);
        assert_eq!(r.seed, None);
        assert_eq!(r.allow_noncommercial, None);
    }

    #[test]
    fn needs_a_prompt_or_a_mood() {
        let f = MakeForm { prompt: "  ".into(), ..MakeForm::default() };
        assert_eq!(f.request(false), Err("Describe the sound or pick a mood.".into()));
        let f = MakeForm { mood: "calm".into(), ..MakeForm::default() };
        assert_eq!(f.request(false).unwrap().mood.as_deref(), Some("calm"));
    }

    #[test]
    fn tempo_is_a_word_or_a_bpm() {
        let with = |t: &str| MakeForm { tempo: t.into(), ..form() }.request(false);
        assert_eq!(with("").unwrap().tempo, None);
        assert_eq!(with("slow").unwrap().tempo, Some(Tempo::Word(TempoWord::Slow)));
        assert_eq!(with("128").unwrap().tempo, Some(Tempo::Bpm(128)));
        assert_eq!(with("300"), Err("A tempo is 40 to 220 bpm.".into()));
    }

    #[test]
    fn an_empty_seed_is_a_new_take_and_a_copied_one_is_kept() {
        assert_eq!(form().request(false).unwrap().seed, None);
        assert_eq!(MakeForm { seed: " 42 ".into(), ..form() }.request(false).unwrap().seed, Some(42));
    }

    fn made() -> crate::domain::Song {
        crate::domain::Song {
            prompt: "warm pads".into(),
            mood: Some("calm".into()),
            genre: Some("lo-fi".into()),
            energy: Some("low".into()),
            seconds: 179.8,
            bpm: Some(93),
            seed: 1234,
            vocals: false,
            ..crate::domain::testing::song("s1")
        }
    }

    #[test]
    fn from_song_copies_the_settings_and_the_seed() {
        let f = MakeForm::from_song(&made());
        assert_eq!((f.prompt.as_str(), f.mood.as_str(), f.genre.as_str(), f.energy.as_str()), ("warm pads", "calm", "lo-fi", "low"));
        assert_eq!(f.seconds, 180, "the nearest length chip");
        assert_eq!(f.tempo, "93");
        assert_eq!(f.seed, "1234");
        let r = f.request(false).unwrap();
        assert_eq!(r.seed, Some(1234));
        assert_eq!(r.energy, Some(crate::domain::request::Energy::Low));
    }

    #[test]
    fn from_song_keeps_the_words_sung_and_works_for_a_mood_only_song() {
        let sung = crate::domain::Song { vocals: true, lyrics: Some("[verse]\nhi".into()), vocal_language: Some("pt".into()), ..made() };
        let f = MakeForm::from_song(&sung);
        assert!(f.vocals);
        assert_eq!((f.lyrics.as_str(), f.language.as_str()), ("[verse]\nhi", "pt"));
        let mood_only = crate::domain::Song { prompt: String::new(), bpm: None, seconds: 41.0, ..made() };
        let f = MakeForm::from_song(&mood_only);
        assert_eq!((f.tempo.as_str(), f.seconds), ("", 30));
        assert!(f.request(false).is_ok(), "a mood alone is a valid request");
    }

    #[test]
    fn tempo_options_include_a_bpm_that_is_not_a_chip() {
        let opts = tempo_options("93");
        assert!(opts.iter().any(|(v, l)| v == "93" && l == "93 bpm"));
        assert_eq!(tempo_options("90").len(), TEMPOS.len(), "a chip value adds nothing");
    }

    #[test]
    fn lyrics_settings_go_only_with_vocals() {
        let f = MakeForm { lyrics: "[verse]\nhi".into(), language: "en".into(), lyrics_by: "claude".into(), ..form() };
        let r = f.request(false).unwrap();
        assert_eq!((r.lyrics, r.vocal_language, r.lyrics_by), (None, None, None));
        let r = MakeForm { vocals: true, ..f }.request(false).unwrap();
        assert_eq!(r.vocals, Some(true));
        assert_eq!(r.lyrics.as_deref(), Some("[verse]\nhi"));
        assert_eq!(r.vocal_language.as_deref(), Some("en"));
        assert_eq!(r.lyrics_by.as_deref(), Some("claude"));
    }

    #[test]
    fn empty_lyrics_ask_the_writer() {
        let r = MakeForm { vocals: true, lyrics: "  ".into(), ..form() }.request(false).unwrap();
        assert_eq!(r.lyrics, None);
    }

    #[test]
    fn noncommercial_only_when_allowed() {
        assert_eq!(form().request(true).unwrap().allow_noncommercial, Some(true));
    }
}
