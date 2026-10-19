//! Numbers and words as the web app shows them.

use crate::{
    domain::{Commercial, PlaylistRef, Song, engine::Health},
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

fn engine_name(e: &EngineStatus) -> String {
    e.info.as_ref().map_or_else(|| e.id.clone(), |i| i.name.clone())
}

/// The header's engine line, and whether a song can be made now: the best engine's state, in a few words.
pub fn engine_line(engines: &[EngineStatus]) -> (String, bool) {
    let find = |h: Health| engines.iter().find(|e| e.health == h).map(engine_name);
    if let Some(n) = find(Health::Up) {
        return (format!("{n} ready"), true);
    }
    let line = if let Some(n) = find(Health::Loading) {
        format!("{n} is loading its model")
    } else if let Some(n) = find(Health::Unknown) {
        format!("Checking {n}…")
    } else if let Some(n) = find(Health::Incompatible) {
        format!("{n} needs an update")
    } else if let [one] = engines {
        format!("{} is not running", engine_name(one))
    } else if engines.is_empty() {
        "No engine set up".into()
    } else {
        "No engine is running".into()
    };
    (line, false)
}

/// One engine in the header's engine panel: its name, what it is doing (and what to do about it), and what it can do.
pub fn engine_card(e: &EngineStatus) -> (String, String, Vec<String>) {
    let state = match e.health {
        Health::Up => "Ready to make songs.",
        Health::Loading => "Started, and loading its model; songs can be made in a minute or two.",
        Health::Unknown => "Checking whether it answers.",
        Health::Down => "Not answering. Start it on the Mac with `just engine`.",
        Health::Incompatible => "It speaks another version of the engine protocol; update maio-engine.",
    };
    let facts = e.info.as_ref().map_or_else(Vec::new, |i| {
        let c = &i.capabilities;
        let licence = match i.licence.commercial {
            Commercial::Yes => String::new(),
            Commercial::Limited => " (limited commercial use)".into(),
            Commercial::No => " (non-commercial use only)".into(),
        };
        vec![
            format!("Version {}", i.version),
            format!("Model {}", if i.model.is_empty() { "not loaded" } else { &i.model }),
            format!("Lyrics model {}", i.lm_model.as_deref().unwrap_or("loads when first needed")),
            format!("Songs of {} to {}", length(c.min_seconds.into()), length(c.max_seconds.into())),
            if c.vocals { "Sings, with lyrics" } else { "Instrumental only" }.to_string(),
            format!("A 3-minute song takes about {}", length(180.0 * f64::from(i.realtime_factor))),
            format!("Licence {}{licence}", i.licence.name),
        ]
    });
    (engine_name(e), state.into(), facts)
}

/// The engine panel's live queue: Maio's own jobs, as the "Being made" list shows them.
pub fn queue_line(making: usize, waiting: usize) -> String {
    match (making, waiting) {
        (0, 0) => "Nothing being made".into(),
        (m, 0) => format!("{m} song{} being made", if m == 1 { "" } else { "s" }),
        (m, w) => format!("{m} song{} being made, {w} waiting", if m == 1 { "" } else { "s" }),
    }
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
    fn engine_line_says_which_engine_is_ready_and_why_not() {
        use crate::domain::engine::Health;
        use crate::ui::api::EngineStatus;
        let e = |id: &str, health| EngineStatus { id: id.into(), info: None, health };
        assert_eq!(engine_line(&[e("ace-step", Health::Up)]), ("ace-step ready".into(), true));
        assert_eq!(engine_line(&[e("ace-step", Health::Loading)]), ("ace-step is loading its model".into(), false));
        assert_eq!(engine_line(&[e("a", Health::Down), e("b", Health::Up)]), ("b ready".into(), true));
        assert_eq!(engine_line(&[e("ace-step", Health::Down)]), ("ace-step is not running".into(), false));
        assert_eq!(engine_line(&[e("a", Health::Down), e("b", Health::Down)]), ("No engine is running".into(), false));
        assert_eq!(engine_line(&[e("ace-step", Health::Unknown)]), ("Checking ace-step…".into(), false));
        assert_eq!(engine_line(&[e("ace-step", Health::Incompatible)]), ("ace-step needs an update".into(), false));
        assert_eq!(engine_line(&[]), ("No engine set up".into(), false));
    }

    #[test]
    fn an_engine_card_says_what_it_is_doing_and_what_it_can_do() {
        use crate::domain::engine::{Caps, EngineInfo, Health};
        use crate::domain::{Commercial, Licence};
        use crate::ui::api::EngineStatus;
        let down = EngineStatus { id: "ace-step".into(), info: None, health: Health::Down };
        assert_eq!(engine_card(&down), ("ace-step".into(), "Not answering. Start it on the Mac with `just engine`.".into(), vec![]));
        let info = EngineInfo {
            id: "ace-step".into(),
            name: "ACE-Step 1.5".into(),
            version: "1.5.0".into(),
            licence: Licence { name: "Apache-2.0".into(), commercial: Commercial::Yes, note: String::new() },
            capabilities: Caps { min_seconds: 10, max_seconds: 600, vocals: true, tempo: true, concurrency: 1 },
            realtime_factor: 0.25,
            model: "acestep-v15-turbo".into(),
            lm_model: None,
            models: vec![],
            queued: 1,
            running: 1,
        };
        let up = EngineStatus { id: "ace-step".into(), info: Some(info), health: Health::Up };
        let (name, state, facts) = engine_card(&up);
        assert_eq!((name.as_str(), state.as_str()), ("ACE-Step 1.5", "Ready to make songs."));
        assert_eq!(
            facts,
            [
                "Version 1.5.0",
                "Model acestep-v15-turbo",
                "Lyrics model loads when first needed",
                "Songs of 10 s to 10 min",
                "Sings, with lyrics",
                "A 3-minute song takes about 45 s",
                "Licence Apache-2.0"
            ]
        );
        let loading = EngineStatus { health: Health::Loading, ..down.clone() };
        assert!(engine_card(&loading).1.contains("loading its model"));
        let old = EngineStatus { health: Health::Incompatible, ..down };
        assert!(engine_card(&old).1.contains("update maio-engine"));
    }

    #[test]
    fn the_queue_line_counts_songs_being_made_and_waiting() {
        assert_eq!(queue_line(0, 0), "Nothing being made");
        assert_eq!(queue_line(1, 0), "1 song being made");
        assert_eq!(queue_line(1, 2), "1 song being made, 2 waiting");
    }
}
