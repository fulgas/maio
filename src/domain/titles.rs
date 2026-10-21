//! Titles in layers (arc42 8.2), layers 3 and 4: from the lyrics, and from word lists picked by the seed.

use crate::domain::MAX_TITLE;

/// At most `max` characters, cut on a character boundary, then trimmed.
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

/// The first line of the first `[chorus]`, else the first lyric line, in title case (layer 3).
pub fn from_lyrics(lyrics: &str) -> Option<String> {
    let lines: Vec<&str> = lyrics.lines().map(str::trim).collect();
    let is_tag = |l: &&str| l.starts_with('[');
    let chorus = lines.iter().position(|l| l.to_lowercase().starts_with("[chorus"));
    let pick = chorus
        .and_then(|i| lines[i + 1..].iter().take_while(|l| !is_tag(l)).find(|l| !l.is_empty()))
        .or_else(|| lines.iter().find(|l| !l.is_empty() && !is_tag(l)))?;
    let title = title_case(&trim_to(pick, MAX_TITLE));
    (!title.is_empty()).then_some(title)
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

/// Words chosen by mood and genre, picked by the seed (layer 4): "Low Tide Static", "Midnight Rhodes".
pub fn from_words(mood: Option<&str>, genre: Option<&str>, seed: i64) -> String {
    let (words, nouns) = (mood_words(mood), genre_nouns(genre));
    let s = seed as u64;
    let a = words[(s % words.len() as u64) as usize];
    let n = nouns[((s / 7) % nouns.len() as u64) as usize];
    if (s / 49).is_multiple_of(3) {
        let extra = GENERAL_NOUNS[((s / 3) % GENERAL_NOUNS.len() as u64) as usize];
        format!("{a} {n} {extra}")
    } else {
        format!("{a} {n}")
    }
}

/// A word title the library does not have yet: the seed first, then up to 64 next seeds.
pub fn pick_words(mood: Option<&str>, genre: Option<&str>, seed: i64, taken: impl Fn(&str) -> bool) -> String {
    let mut title = from_words(mood, genre, seed);
    for next in 1..=64 {
        if !taken(&title) {
            break;
        }
        title = from_words(mood, genre, seed.wrapping_add(next));
    }
    title
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn a_title_from_lyrics_is_the_first_chorus_line_in_title_case() {
        let l = "[verse]\nwalking home alone\n\n[Chorus]\n  night bus to nowhere  \nsecond line\n";
        assert_eq!(from_lyrics(l).as_deref(), Some("Night Bus To Nowhere"));
        assert_eq!(from_lyrics("[verse]\n\nfirst sung line\n").as_deref(), Some("First Sung Line"), "no chorus: the first lyric line");
        assert_eq!(from_lyrics("[verse]\n[chorus]\n"), None);
        assert_eq!(from_lyrics(&format!("[chorus]\n{}", "wordy".repeat(30))).unwrap().chars().count(), 60);
        assert_eq!(from_lyrics("[chorus]\nnaïve café ünder").as_deref(), Some("Naïve Café Ünder"));
    }

    #[test]
    fn word_titles_follow_mood_genre_and_seed() {
        let a = from_words(Some("calm"), Some("jazz"), 7);
        assert_eq!(a, from_words(Some("calm"), Some("jazz"), 7), "same seed, same title");
        assert!((0..20).map(|s| from_words(Some("calm"), Some("jazz"), s)).collect::<HashSet<_>>().len() > 10);
        assert!(!from_words(None, None, 3).is_empty(), "no mood or genre uses the general lists");
        assert!(!from_words(Some("not-a-mood"), Some("not-a-genre"), -5).is_empty(), "negative seeds work");
        assert!(a.split(' ').all(|w| w.chars().next().unwrap().is_uppercase()));
    }

    #[test]
    fn word_titles_skip_titles_the_library_has() {
        let first = pick_words(Some("calm"), None, 11, |_| false);
        assert_eq!(first, from_words(Some("calm"), None, 11));
        let second = pick_words(Some("calm"), None, 11, |t| t == first);
        assert_ne!(first, second, "the next seed is tried");
        assert!(!pick_words(Some("calm"), None, 11, |_| true).is_empty(), "when every try is taken, the last one stands");
    }

    #[test]
    fn trim_to_cuts_on_characters_and_trims() {
        assert_eq!(trim_to("  é".repeat(3).as_str(), 6), "é  é");
        assert_eq!(trim_to("abc ", 10), "abc");
    }
}
