//! The Make form (arc42 8.2, 8.5): what the person picked, turned into a song request.

use crate::{
    domain::{
        engine::Health,
        request::{Energy, SongRequest, Tempo},
    },
    ui::api::EngineStatus,
};

/// The length slider: 0:15 to 10:00 in steps of 15 seconds, the lengths the API takes (8.2).
pub const SHORTEST: u32 = 15;
pub const LONGEST: u32 = 600;
pub const STEP: u32 = 15;
/// The tempo slider: 40 to 220 BPM in steps of 1, the tempos the API takes (8.2).
pub const SLOWEST: u32 = 40;
pub const FASTEST: u32 = 220;
/// Where the slider rests while the tempo is on Auto.
pub const AUTO_AT: u32 = 120;
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
    /// A BPM, or none for Auto: the engine decides.
    pub tempo: Option<u32>,
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
            tempo: None,
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
            tempo: self.tempo.map(Tempo::Bpm),
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

/// What "Make one like this" puts in the form (arc42 8.5): the song's settings, its words and its seed.
impl MakeForm {
    pub fn from_song(s: &crate::domain::Song) -> Self {
        Self {
            prompt: s.prompt.clone(),
            mood: s.mood.clone().unwrap_or_default(),
            genre: s.genre.clone().unwrap_or_default(),
            energy: s.energy.clone().unwrap_or_default(),
            seconds: slider_seconds(s.seconds),
            tempo: s.bpm.filter(|b| (SLOWEST..=FASTEST).contains(b)),
            seed: s.seed.to_string(),
            vocals: s.vocals,
            lyrics: if s.vocals { s.lyrics.clone().unwrap_or_default() } else { String::new() },
            language: if s.vocals { s.vocal_language.clone().unwrap_or_default() } else { String::new() },
            lyrics_by: String::new(),
        }
    }
}

/// A song's length on the slider: the nearest 15 seconds, within 0:15 to 10:00.
pub fn slider_seconds(seconds: f64) -> u32 {
    ((seconds / f64::from(STEP)).round() as u32 * STEP).clamp(SHORTEST, LONGEST)
}

/// Where the slider ends: the longest song a ready engine can make, on a step; 10:00 while none is ready.
pub fn longest(engines: &[EngineStatus]) -> u32 {
    engines
        .iter()
        .filter(|e| e.health == Health::Up)
        .filter_map(|e| e.info.as_ref())
        .map(|i| i.capabilities.max_seconds / STEP * STEP)
        .max()
        .map_or(LONGEST, |m| m.clamp(SHORTEST, LONGEST))
}

/// A form for the Make box to take up: "Make one like this" sets it, the Make box empties it.
#[derive(Clone, Copy)]
pub struct Draft(pub leptos::prelude::RwSignal<Option<MakeForm>>);

#[cfg(test)]
mod tests {
    use super::*;

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
    fn tempo_is_a_bpm_or_auto() {
        let with = |t| MakeForm { tempo: t, ..form() }.request(false).unwrap().tempo;
        assert_eq!(with(None), None, "Auto: the engine decides");
        assert_eq!(with(Some(96)), Some(Tempo::Bpm(96)));
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
        assert_eq!(f.seconds, 180, "the nearest step of the length slider");
        assert_eq!(f.tempo, Some(93));
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
        assert_eq!((f.tempo, f.seconds), (None, 45), "no tempo: Auto");
        assert!(f.request(false).is_ok(), "a mood alone is a valid request");
    }

    #[test]
    fn a_length_snaps_to_the_slider() {
        assert_eq!((slider_seconds(20.0), slider_seconds(22.6), slider_seconds(179.8)), (15, 30, 180), "the nearest 15 seconds");
        assert_eq!((slider_seconds(3.0), slider_seconds(900.0)), (15, 600), "within 0:15 to 10:00");
    }

    #[test]
    fn the_longest_length_is_what_the_ready_engines_can_make() {
        use crate::domain::engine::Health;
        use crate::ui::api::EngineStatus;
        let engine = |max_seconds, health| {
            let mut info = crate::domain::testing::engine_info("e", crate::domain::Commercial::Yes);
            info.capabilities.max_seconds = max_seconds;
            EngineStatus { id: "e".into(), info: Some(info), health }
        };
        assert_eq!(longest(&[]), 600, "no engine known: the API's own limit");
        assert_eq!(longest(&[engine(240, Health::Up)]), 240);
        assert_eq!(longest(&[engine(250, Health::Up)]), 240, "on a step of the slider");
        assert_eq!(longest(&[engine(900, Health::Up)]), 600);
        assert_eq!(longest(&[engine(120, Health::Down)]), 600, "only a ready engine counts");
        assert_eq!(longest(&[engine(120, Health::Up), engine(300, Health::Up)]), 300, "the longest any can make");
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
