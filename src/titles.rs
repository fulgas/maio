//! Track titles in layers (SPEC 10, Titles): a person, a text engine, the lyrics, then word lists.

use crate::{request::Normalized, state::AppState, store::Store};

const MAX: usize = 60;

/// At most `max` characters, cut on a character boundary and trimmed.
pub fn trim_to(s: &str, max: usize) -> String {
    s.chars().take(max).collect::<String>().trim().to_string()
}

fn title_case(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect::<String>()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The first line of the first `[chorus]` section, else the first lyric line (SPEC 10, layer 3).
pub fn from_lyrics(lyrics: &str) -> Option<String> {
    let lines: Vec<&str> = lyrics.lines().map(str::trim).collect();
    let is_tag = |l: &&str| l.starts_with('[');
    let chorus = lines.iter().position(|l| l.to_lowercase().starts_with("[chorus"));
    let pick = chorus
        .and_then(|i| lines[i + 1..].iter().take_while(|l| !is_tag(l)).find(|l| !l.is_empty()))
        .or_else(|| lines.iter().find(|l| !l.is_empty() && !is_tag(l)))?;
    let t = title_case(&trim_to(pick, MAX));
    (!t.is_empty()).then_some(t)
}

const GENERAL_WORDS: &[&str] = &["Paper", "Silver", "Hollow", "Distant", "Golden", "Northern", "Open", "Velvet"];
const GENERAL_NOUNS: &[&str] = &["Lanterns", "Rooms", "Signals", "Rivers", "Windows", "Letters", "Harbours", "Echoes"];

fn mood_words(mood: Option<&str>) -> &'static [&'static str] {
    match mood {
        Some("calm") => &["Low Tide", "Still", "Quiet", "Slow", "Soft", "Morning"],
        Some("focused") => &["Clear", "Steady", "Narrow", "Fine", "Single", "Even"],
        Some("upbeat") => &["Bright", "Sunlit", "Quick", "High", "Open Road", "Lucky"],
        Some("playful") => &["Paper Kite", "Bouncing", "Sideways", "Pocket", "Tumbling", "Little"],
        Some("tense") => &["Static", "Cold", "Wired", "Sharp", "Midnight", "Fault"],
        Some("dreamy") => &["Floating", "Hazy", "Moonlit", "Drifting", "Cloud", "Faraway"],
        Some("melancholy") => &["Grey", "Rain", "Fading", "Last", "Empty", "Winter"],
        Some("triumphant") => &["Rising", "Crowned", "Summit", "Burning", "Iron", "Victory"],
        _ => GENERAL_WORDS,
    }
}

fn genre_nouns(genre: Option<&str>) -> &'static [&'static str] {
    match genre {
        Some("lo-fi") => &["Tapes", "Cassettes", "Rooftops", "Cafés", "Static", "Notebooks"],
        Some("ambient") => &["Fields", "Tides", "Horizons", "Shores", "Air", "Light"],
        Some("synthwave") => &["Neon", "Highways", "Grids", "Arcades", "Lasers", "Chrome"],
        Some("jazz") => &["Rhodes", "Smoke", "Corners", "Brass", "Avenues", "Basements"],
        Some("orchestral") => &["Halls", "Overtures", "Banners", "Kingdoms", "Storms", "Cathedrals"],
        Some("rock") => &["Engines", "Amps", "Streets", "Wires", "Garages", "Riots"],
        Some("electronic") => &["Circuits", "Pulses", "Currents", "Codes", "Frequencies", "Machines"],
        _ => GENERAL_NOUNS,
    }
}

/// Words chosen per mood and genre, picked by the seed (SPEC 10, layer 4): "Low Tide Static", "Midnight Rhodes".
pub fn from_words(mood: Option<&str>, genre: Option<&str>, seed: i64) -> String {
    let (words, nouns) = (mood_words(mood), genre_nouns(genre));
    let s = seed as u64;
    let a = words[(s % words.len() as u64) as usize];
    let n = nouns[((s / 7) % nouns.len() as u64) as usize];
    if (s / 49) % 3 == 0 {
        let extra = GENERAL_NOUNS[((s / 3) % GENERAL_NOUNS.len() as u64) as usize];
        format!("{a} {n} {extra}")
    } else {
        format!("{a} {n}")
    }
}

/// A word title the crate does not have yet: the seed first, then the next seeds.
pub fn pick_words(store: &Store, mood: Option<&str>, genre: Option<&str>, seed: i64) -> String {
    let mut title = from_words(mood, genre, seed);
    for next in 1..=64 {
        if !store.title_taken(&title).unwrap_or(false) {
            break;
        }
        title = from_words(mood, genre, seed.wrapping_add(next));
    }
    title
}

/// A title and the layer that gave it.
#[derive(Debug, Clone, PartialEq)]
pub struct Named {
    pub title: String,
    pub source: &'static str,
    pub engine: Option<String>,
    pub description: Option<String>,
}

/// Names a new track: a text engine (when `[naming] engine` is set), the lyrics, else word lists.
pub async fn name(state: &AppState, n: &Normalized, lyrics: Option<&str>) -> Named {
    if let Some(id) = &state.cfg.naming.engine {
        if let Some(te) = state.text_engine(id).filter(|t| t.health() != crate::engine::Health::Down) {
            match crate::text::name_song(&te, n, lyrics, crate::text::NAMING_TIMEOUT).await {
                Ok((title, description)) => {
                    return Named { title, source: "engine", engine: Some(id.clone()), description: Some(description).filter(|d| !d.is_empty()) };
                }
                Err(e) => tracing::warn!(text_engine = %id, "naming: {e:#}"),
            }
        }
    }
    if let Some(title) = lyrics.and_then(from_lyrics) {
        return Named { title, source: "lyrics", engine: None, description: None };
    }
    let title = pick_words(&state.store, n.mood.as_deref(), n.genre.as_deref(), n.seed);
    Named { title, source: "words", engine: None, description: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_from_lyrics_is_the_first_chorus_line_in_title_case() {
        let l = "[verse]\nwalking home alone\n\n[Chorus]\n  night bus to nowhere  \nsecond line\n";
        assert_eq!(from_lyrics(l).as_deref(), Some("Night Bus To Nowhere"));
        assert_eq!(from_lyrics("[verse]\n\nfirst sung line\n").as_deref(), Some("First Sung Line"), "no chorus: first lyric line");
        assert_eq!(from_lyrics("[verse]\n[chorus]\n"), None);
        assert_eq!(from_lyrics(&format!("[chorus]\n{}", "wordy".repeat(30))).unwrap().chars().count(), 60);
        assert_eq!(from_lyrics("[chorus]\nnaïve café ünder").as_deref(), Some("Naïve Café Ünder"));
    }

    #[test]
    fn word_titles_follow_mood_genre_and_seed() {
        let a = from_words(Some("calm"), Some("jazz"), 7);
        assert_eq!(a, from_words(Some("calm"), Some("jazz"), 7), "same seed, same title");
        assert!((0..20).map(|s| from_words(Some("calm"), Some("jazz"), s)).collect::<std::collections::HashSet<_>>().len() > 10);
        assert!(!from_words(None, None, 3).is_empty(), "unknown mood and genre use the general lists");
        assert!(!from_words(Some("not-a-mood"), Some("not-a-genre"), -5).is_empty(), "negative seeds work");
        assert!(a.split(' ').all(|w| w.chars().next().unwrap().is_uppercase()));
    }

    #[test]
    fn word_titles_skip_titles_the_crate_already_has() {
        let s = Store::open_in_memory().unwrap();
        let first = pick_words(&s, Some("calm"), None, 11);
        let mut t = crate::store::Track { title: first.clone(), ..crate::store::tests::sample("a", None, None) };
        t.title_source = "words".into();
        s.insert_track(&t).unwrap();
        let second = pick_words(&s, Some("calm"), None, 11);
        assert_ne!(first, second, "the next seed is tried");
    }
}
