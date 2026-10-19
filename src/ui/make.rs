//! The Make form (SPEC 5, 12): what the person picked, turned into a song request.

use crate::domain::request::{SongRequest, Tempo, TempoWord};

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
            "" | "random" => None,
            s => Some(s.parse::<i64>().map_err(|_| "The seed is a whole number, or empty for a random one.".to_string())?),
        };
        let v = self.vocals;
        Ok(SongRequest {
            prompt,
            mood,
            genre: some(&self.genre),
            energy: None,
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
    fn seed_is_random_or_a_whole_number() {
        let with = |s: &str| MakeForm { seed: s.into(), ..form() }.request(false);
        assert_eq!(with("").unwrap().seed, None);
        assert_eq!(with("random").unwrap().seed, None);
        assert_eq!(with(" 42 ").unwrap().seed, Some(42));
        assert_eq!(with("4.2"), Err("The seed is a whole number, or empty for a random one.".into()));
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
