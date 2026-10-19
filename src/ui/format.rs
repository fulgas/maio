//! Numbers and words as the web app shows them.

use crate::{
    domain::{PlaylistRef, Song, engine::Health},
    ui::api::EngineStatus,
};

/// 184.4 → "3:04".
pub fn clock(seconds: f64) -> String {
    let s = seconds.max(0.0).round() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// A total: "45 s", "58 min", "2 h 31 min".
pub fn length(seconds: f64) -> String {
    let s = seconds.max(0.0).round() as u64;
    match (s / 3600, s % 3600 / 60) {
        (0, 0) => format!("{s} s"),
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

pub fn songs(n: usize) -> String {
    if n == 1 { "1 song".into() } else { format!("{n} songs") }
}

/// Time left on a job.
pub fn eta(eta_s: Option<u32>) -> String {
    match eta_s {
        None => "waiting".into(),
        Some(s) if s < 90 => format!("about {s} s"),
        Some(s) => format!("about {} min", (f64::from(s) / 60.0).round()),
    }
}

/// Time left on a job: the estimate for the whole run, less what progress says is done.
pub fn time_left(eta_s: Option<u32>, progress: f32) -> String {
    eta(eta_s.map(|s| (s as f32 * (1.0 - progress.clamp(0.0, 1.0))).round() as u32))
}

/// "calm, D minor": the mood and key a song row shows.
pub fn meta(song: &Song) -> String {
    [song.mood.as_deref(), song.key.as_deref()].into_iter().flatten().collect::<Vec<_>>().join(", ")
}

/// "In A, B and C": the label on a song's playlist marks.
pub fn in_playlists(playlists: &[PlaylistRef]) -> String {
    let names: Vec<&str> = playlists.iter().map(|p| p.name.as_str()).collect();
    match names.as_slice() {
        [] => "In no playlist".into(),
        [one] => format!("In {one}"),
        [rest @ .., last] => format!("In {} and {last}", rest.join(", ")),
    }
}

/// The header's engine line, and whether a song can be made now.
pub fn engine_line(engines: &[EngineStatus]) -> (String, bool) {
    let name = |e: &EngineStatus| e.info.as_ref().map_or_else(|| e.id.clone(), |i| i.name.clone());
    if let Some(e) = engines.iter().find(|e| e.health == Health::Up) {
        return (format!("{} ready", name(e)), true);
    }
    if let Some(e) = engines.iter().find(|e| e.health == Health::Loading) {
        return (format!("{} is loading", name(e)), false);
    }
    ("No engine is ready".into(), false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{PlaylistRef, testing};

    #[test]
    fn clock_is_minutes_and_seconds() {
        assert_eq!(clock(0.0), "0:00");
        assert_eq!(clock(184.4), "3:04");
        assert_eq!(clock(600.0), "10:00");
        assert_eq!(clock(-3.0), "0:00");
    }

    #[test]
    fn length_is_a_total() {
        assert_eq!(length(45.0), "45 s");
        assert_eq!(length(3480.0), "58 min");
        assert_eq!(length(9060.0), "2 h 31 min");
        assert_eq!(length(7200.0), "2 h");
    }

    #[test]
    fn counts_and_etas() {
        assert_eq!(songs(1), "1 song");
        assert_eq!(songs(18), "18 songs");
        assert_eq!(eta(Some(30)), "about 30 s");
        assert_eq!(eta(Some(150)), "about 3 min");
        assert_eq!(eta(None), "waiting");
    }

    #[test]
    fn time_left_counts_down_with_progress() {
        assert_eq!(time_left(Some(120), 0.0), "about 2 min");
        assert_eq!(time_left(Some(120), 0.75), "about 30 s");
        assert_eq!(time_left(Some(120), 1.5), "about 0 s", "progress past the end is not negative");
        assert_eq!(time_left(None, 0.5), "waiting");
    }

    #[test]
    fn meta_joins_mood_and_key() {
        let mut s = testing::song("s1");
        s.mood = Some("calm".into());
        s.key = Some("D minor".into());
        assert_eq!(meta(&s), "calm, D minor");
        s.mood = None;
        assert_eq!(meta(&s), "D minor");
        s.key = None;
        assert_eq!(meta(&s), "");
    }

    #[test]
    fn in_playlists_names_them() {
        let p = |n: &str| PlaylistRef { id: n.into(), name: n.into() };
        assert_eq!(in_playlists(&[]), "In no playlist");
        assert_eq!(in_playlists(&[p("Rainy jazz")]), "In Rainy jazz");
        assert_eq!(in_playlists(&[p("A"), p("B"), p("C")]), "In A, B and C");
    }

    #[test]
    fn engine_line_says_which_engine_is_ready() {
        use crate::domain::engine::Health;
        use crate::ui::api::EngineStatus;
        let e = |id: &str, health| EngineStatus { id: id.into(), info: None, health };
        assert_eq!(engine_line(&[e("ace-step", Health::Up)]), ("ace-step ready".into(), true));
        assert_eq!(engine_line(&[e("ace-step", Health::Loading)]), ("ace-step is loading".into(), false));
        assert_eq!(engine_line(&[e("a", Health::Down), e("b", Health::Up)]), ("b ready".into(), true));
        assert_eq!(engine_line(&[e("a", Health::Down)]), ("No engine is ready".into(), false));
        assert_eq!(engine_line(&[]), ("No engine is ready".into(), false));
    }
}
