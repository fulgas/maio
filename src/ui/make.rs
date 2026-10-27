//! The Make form (arc42 8.2, 8.5): what the person picked, turned into a song request.

use crate::{
    domain::{
        engine::Health,
        request::{Auto, Energy, SongRequest, Tempo, Vocals, Voice, engine_prompt},
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
/// The settings panel's lists (arc42 8.5): (value, label), Auto first; Auto sends nothing, or `"auto"` for vocals.
pub const ENERGIES: [(&str, &str); 4] = [("", "Auto"), ("low", "Low"), ("medium", "Medium"), ("high", "High")];
pub const VOCALS: [(&str, &str); 3] = [("", "Auto"), ("instrumental", "Instrumental"), ("vocals", "Vocals")];
pub const VOICES: [(&str, &str); 4] = [("", "Auto"), ("female", "Female"), ("male", "Male"), ("duet", "Duet")];
/// "Sung in": (code, label); Auto is the server's default language.
pub const LANGUAGES: [(&str, &str); 10] = [
    ("", "Auto"),
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

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MakeForm {
    pub prompt: String,
    pub mood: String,
    pub genre: String,
    pub energy: String,
    /// A length, or none for Auto: Maio picks one.
    pub seconds: Option<u32>,
    /// A BPM, or none for Auto: the engine decides.
    pub tempo: Option<u32>,
    pub seed: String,
    /// `""` for Auto (Maio picks), `"instrumental"` or `"vocals"`.
    pub vocals: String,
    /// `""` for Auto, or a voice: only with vocals.
    pub voice: String,
    pub lyrics: String,
    pub language: String,
    pub lyrics_by: String,
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
        let v = self.sings();
        Ok(SongRequest {
            prompt,
            mood,
            genre: some(&self.genre),
            energy: self.energy(),
            seconds: self.seconds,
            tempo: self.tempo.map(Tempo::Bpm),
            vocals: Some(match self.vocals.as_str() {
                "" => Vocals::Auto(Auto::Auto),
                _ => Vocals::Set(v),
            }),
            voice: self.voice(),
            lyrics: if v { some(&self.lyrics) } else { None },
            vocal_language: if v { some(&self.language) } else { None },
            lyrics_by: if v { some(&self.lyrics_by) } else { None },
            engine: None,
            seed,
            allow_noncommercial: allow_noncommercial.then_some(true),
        })
    }

    pub fn sings(&self) -> bool {
        self.vocals == "vocals"
    }

    fn energy(&self) -> Option<Energy> {
        match self.energy.as_str() {
            "low" => Some(Energy::Low),
            "medium" => Some(Energy::Medium),
            "high" => Some(Energy::High),
            _ => None,
        }
    }

    fn voice(&self) -> Option<Voice> {
        Voice::parse(&self.voice).filter(|_| self.sings())
    }

    /// The empty prompt's placeholder: what the settings alone send to the engine, or None while none of them is set.
    pub fn placeholder(&self) -> Option<String> {
        let words = [!self.mood.is_empty(), !self.genre.is_empty(), !self.energy.is_empty(), self.tempo.is_some(), !self.vocals.is_empty()];
        words.contains(&true).then(|| {
            engine_prompt(
                None,
                some(&self.genre).as_deref(),
                some(&self.mood).as_deref(),
                self.energy(),
                self.tempo,
                self.vocals != "instrumental",
                self.voice(),
            )
        })
    }

    /// How many settings are not on Auto: the count on the settings button.
    pub fn count(&self) -> usize {
        let sings = self.sings();
        [
            !self.mood.is_empty(),
            !self.genre.is_empty(),
            !self.energy.is_empty(),
            self.tempo.is_some(),
            self.seconds.is_some(),
            !self.seed.trim().is_empty(),
            !self.vocals.is_empty(),
            sings && !self.voice.is_empty(),
            sings && !self.language.is_empty(),
        ]
        .into_iter()
        .filter(|set| *set)
        .count()
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
            seconds: Some(slider_seconds(s.seconds)),
            tempo: s.bpm.filter(|b| (SLOWEST..=FASTEST).contains(b)),
            seed: s.seed.to_string(),
            vocals: if s.vocals { "vocals" } else { "instrumental" }.into(),
            voice: if s.vocals { s.voice.clone().unwrap_or_default() } else { String::new() },
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
    fn a_prompt_alone_leaves_every_setting_on_auto() {
        let r = form().request(false).unwrap();
        assert_eq!(r.prompt.as_deref(), Some("warm pads"));
        assert_eq!((r.seconds, r.tempo, r.seed), (None, None, None), "Maio picks the length, the engine the tempo, a new seed");
        assert_eq!(r.vocals, Some(Vocals::Auto(Auto::Auto)), "Maio picks instrumental or vocals");
        assert_eq!((r.voice, r.lyrics), (None, None));
        assert_eq!(r.allow_noncommercial, None);
        assert_eq!(form().count(), 0, "nothing set");
    }

    #[test]
    fn length_is_a_time_or_auto() {
        assert_eq!(MakeForm { seconds: Some(150), ..form() }.request(false).unwrap().seconds, Some(150));
    }

    #[test]
    fn vocals_are_auto_instrumental_or_sung_and_a_voice_goes_only_with_vocals() {
        let with = |vocals: &str, voice: &str| MakeForm { vocals: vocals.into(), voice: voice.into(), ..form() }.request(false).unwrap();
        assert_eq!(with("instrumental", "male").vocals, Some(Vocals::Set(false)));
        assert_eq!(with("instrumental", "male").voice, None);
        assert_eq!(with("", "male").voice, None, "on Auto there is no voice to pick");
        let r = with("vocals", "duet");
        assert_eq!((r.vocals, r.voice), (Some(Vocals::Set(true)), Some(Voice::Duet)));
        assert_eq!(with("vocals", "").voice, None, "Auto voice");
    }

    #[test]
    fn the_placeholder_is_what_the_settings_alone_send() {
        assert_eq!(MakeForm::default().placeholder(), None, "nothing set: the sample description");
        let f = MakeForm { genre: "ambient".into(), mood: "calm".into(), tempo: Some(96), vocals: "vocals".into(), voice: "female".into(), ..form() };
        assert_eq!(f.placeholder().as_deref(), Some("ambient, calm mood, 96 bpm, female vocals"));
        assert_eq!(MakeForm { vocals: "instrumental".into(), ..MakeForm::default() }.placeholder().as_deref(), Some("instrumental"));
    }

    #[test]
    fn the_count_is_the_settings_that_are_not_on_auto() {
        let f = MakeForm {
            mood: "calm".into(),
            energy: "low".into(),
            tempo: Some(96),
            seconds: Some(60),
            seed: "42".into(),
            vocals: "instrumental".into(),
            voice: "male".into(),
            language: "pt".into(),
            ..form()
        };
        assert_eq!(f.count(), 6, "a voice and a language count only with vocals");
        assert_eq!(MakeForm { vocals: "vocals".into(), ..f }.count(), 8);
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
        assert_eq!(f.seconds, Some(180), "the nearest step of the length slider");
        assert_eq!(f.vocals, "instrumental");
        assert_eq!(f.tempo, Some(93));
        assert_eq!(f.seed, "1234");
        let r = f.request(false).unwrap();
        assert_eq!(r.seed, Some(1234));
        assert_eq!(r.energy, Some(crate::domain::request::Energy::Low));
    }

    #[test]
    fn from_song_keeps_the_words_sung_and_works_for_a_mood_only_song() {
        let sung = crate::domain::Song {
            vocals: true,
            voice: Some("male".into()),
            lyrics: Some("[verse]\nhi".into()),
            vocal_language: Some("pt".into()),
            ..made()
        };
        let f = MakeForm::from_song(&sung);
        assert_eq!((f.vocals.as_str(), f.voice.as_str()), ("vocals", "male"));
        assert_eq!((f.lyrics.as_str(), f.language.as_str()), ("[verse]\nhi", "pt"));
        let mood_only = crate::domain::Song { prompt: String::new(), bpm: None, seconds: 41.0, ..made() };
        let f = MakeForm::from_song(&mood_only);
        assert_eq!((f.tempo, f.seconds), (None, Some(45)), "no tempo: Auto");
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
        let r = MakeForm { vocals: "vocals".into(), ..f }.request(false).unwrap();
        assert_eq!(r.vocals, Some(Vocals::Set(true)));
        assert_eq!(r.lyrics.as_deref(), Some("[verse]\nhi"));
        assert_eq!(r.vocal_language.as_deref(), Some("en"));
        assert_eq!(r.lyrics_by.as_deref(), Some("claude"));
    }

    #[test]
    fn empty_lyrics_ask_the_writer() {
        let r = MakeForm { vocals: "vocals".into(), lyrics: "  ".into(), ..form() }.request(false).unwrap();
        assert_eq!(r.lyrics, None);
    }

    #[test]
    fn noncommercial_only_when_allowed() {
        assert_eq!(form().request(true).unwrap().allow_noncommercial, Some(true));
    }
}
