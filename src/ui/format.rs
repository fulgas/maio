//! Numbers and words as the web app shows them.

use crate::{
    domain::{
        Commercial, PlaylistRef, Song,
        engine::Health,
        relay::{RelayState, RelayStatus, RelayStream},
    },
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

/// What a song's playlist marks say on screen: the first playlist's name, and how many more ("Rainy jazz +2").
pub fn playlist_mark(playlists: &[PlaylistRef]) -> String {
    match playlists {
        [] => String::new(),
        [one] => one.name.clone(),
        [first, rest @ ..] => format!("{} +{}", first.name, rest.len()),
    }
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
        Health::Down => "Not answering. Check that its server is running and reachable at the address in the configuration file.",
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

/// The header's MediaMTX line, and whether all is well: it answers and every live playlist reaches it.
pub fn relay_line(r: &RelayStatus) -> (String, bool) {
    let missing: Vec<&RelayStream> = r.streams.iter().filter(|s| !s.arriving).collect();
    match (r.state, missing.as_slice()) {
        (RelayState::Checking, _) => ("Checking MediaMTX…".into(), false),
        (RelayState::Down, _) => ("MediaMTX not answering".into(), false),
        (RelayState::Up, []) => ("MediaMTX ready".into(), true),
        (RelayState::Up, [one]) => (format!("MediaMTX: {} not arriving", one.playlist.name), false),
        (RelayState::Up, many) => (format!("MediaMTX: {} streams not arriving", many.len()), false),
    }
}

/// Why a live address cannot be copied now, if it cannot: RTSP, SRT and RTMP come through MediaMTX, so not while it
/// is not answering. While Maio is still checking it after a start they can, as the header warns of nothing then.
pub fn address_blocked(kind: &str, relay: Option<RelayState>) -> Option<&'static str> {
    (matches!(kind, "RTSP" | "SRT" | "RTMP") && relay == Some(RelayState::Down)).then_some("MediaMTX is not answering")
}

/// A live playlist on the MediaMTX panel: its name, then facts about it.
pub type RelayStreamCard = (String, Vec<String>);

/// The MediaMTX panel: what it is doing, facts about it, then each live playlist on it.
pub fn relay_card(r: &RelayStatus) -> (String, Vec<String>, Vec<RelayStreamCard>) {
    let state = match r.state {
        RelayState::Up => "Relaying the live playlists over RTSP, SRT and RTMP.",
        RelayState::Checking => "Checking whether it answers.",
        RelayState::Down => "Not answering. Is it running? `docker compose ps` lists it.",
    };
    let mut facts = Vec::new();
    if r.state == RelayState::Up {
        facts.extend(r.version.as_ref().map(|v| format!("Version {v}")));
        facts.extend(r.up_s.map(|s| format!("Up for {}", length(s as f64))));
    }
    facts.extend([format!("RTSP {}", r.rtsp), format!("SRT {}", r.srt), format!("RTMP {}", r.rtmp)]);
    let streams = r
        .streams
        .iter()
        .map(|s| {
            let mut f = vec![if s.arriving {
                s.arriving_s.map_or_else(|| "Arriving".to_string(), |t| format!("Arriving for {}", length(t as f64)))
            } else {
                "Not arriving: its audio is not reaching MediaMTX".to_string()
            }];
            if s.arriving {
                f.extend(s.format.clone());
            }
            let n = s.readers.rtsp + s.readers.srt + s.readers.rtmp;
            f.push(if n == 0 {
                "No readers".to_string()
            } else {
                let by: Vec<String> = [(s.readers.rtsp, "RTSP"), (s.readers.srt, "SRT"), (s.readers.rtmp, "RTMP")]
                    .into_iter()
                    .filter(|(c, _)| *c > 0)
                    .map(|(c, k)| format!("{c} {k}"))
                    .collect();
                format!("{n} reader{}: {}", if n == 1 { "" } else { "s" }, by.join(", "))
            });
            if s.arriving {
                f.push(format!("{} sent", data(s.bytes_sent)));
            }
            if s.frames_in_error > 0 {
                f.push(format!("{} broken frame{}", s.frames_in_error, if s.frames_in_error == 1 { "" } else { "s" }));
            }
            (s.playlist.name.clone(), f)
        })
        .collect();
    (state.into(), facts, streams)
}

/// "12.3 MB": an amount of data, short.
pub fn data(bytes: u64) -> String {
    let b = bytes as f64;
    match bytes {
        0..1_000 => format!("{bytes} B"),
        1_000..1_000_000 => format!("{:.0} KB", b / 1e3),
        1_000_000..1_000_000_000 => format!("{:.1} MB", b / 1e6),
        _ => format!("{:.1} GB", b / 1e9),
    }
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
    use crate::domain::relay::{RelayReaders, RelayState, RelayStatus, RelayStream};
    use crate::domain::{PlaylistRef, testing};

    fn stream(name: &str, arriving: bool, readers: RelayReaders) -> RelayStream {
        RelayStream {
            playlist: PlaylistRef { id: name.into(), name: name.into() },
            arriving,
            arriving_s: arriving.then_some(720),
            format: Some("AAC 48 kHz stereo".into()),
            readers,
            bytes_sent: 12_300_000,
            frames_in_error: 0,
        }
    }

    fn relay(state: RelayState, streams: Vec<RelayStream>) -> RelayStatus {
        RelayStatus {
            state,
            version: Some("v1.21.2".into()),
            up_s: Some(11_400),
            rtsp: "localhost:8555".into(),
            srt: "localhost:8891".into(),
            rtmp: "localhost:1936".into(),
            streams,
        }
    }

    #[test]
    fn the_relay_line_says_whether_mediamtx_carries_every_live_playlist() {
        let none = RelayReaders { rtsp: 0, srt: 0, rtmp: 0 };
        assert_eq!(relay_line(&relay(RelayState::Up, vec![])), ("MediaMTX ready".to_string(), true));
        assert_eq!(relay_line(&relay(RelayState::Checking, vec![])), ("Checking MediaMTX…".to_string(), false));
        assert_eq!(relay_line(&relay(RelayState::Down, vec![])), ("MediaMTX not answering".to_string(), false));
        let one = relay(RelayState::Up, vec![stream("Late night coding", false, none), stream("Jazz", true, none)]);
        assert_eq!(relay_line(&one), ("MediaMTX: Late night coding not arriving".to_string(), false));
        let two = relay(RelayState::Up, vec![stream("A", false, none), stream("B", false, none)]);
        assert_eq!(relay_line(&two), ("MediaMTX: 2 streams not arriving".to_string(), false));
    }

    #[test]
    fn only_the_relay_addresses_are_blocked_and_only_while_mediamtx_is_not_answering() {
        for kind in ["RTSP", "SRT", "RTMP"] {
            assert_eq!(address_blocked(kind, Some(RelayState::Down)), Some("MediaMTX is not answering"));
            assert_eq!(address_blocked(kind, Some(RelayState::Up)), None);
            assert_eq!(address_blocked(kind, Some(RelayState::Checking)), None, "not known to be down yet");
            assert_eq!(address_blocked(kind, None), None);
        }
        for kind in ["MP3", "AAC", "HLS"] {
            assert_eq!(address_blocked(kind, Some(RelayState::Down)), None, "{kind} does not need MediaMTX");
        }
    }

    #[test]
    fn the_relay_card_lists_mediamtx_then_each_live_playlist() {
        let readers = RelayReaders { rtsp: 1, srt: 0, rtmp: 1 };
        let mut broken = stream("Gone", false, RelayReaders { rtsp: 0, srt: 0, rtmp: 0 });
        broken.frames_in_error = 2;
        let (state, facts, streams) = relay_card(&relay(RelayState::Up, vec![stream("Late night coding", true, readers), broken]));
        assert_eq!(state, "Relaying the live playlists over RTSP, SRT and RTMP.");
        assert_eq!(facts, ["Version v1.21.2", "Up for 3 h 10 min", "RTSP localhost:8555", "SRT localhost:8891", "RTMP localhost:1936"]);
        assert_eq!(
            streams[0],
            (
                "Late night coding".to_string(),
                vec!["Arriving for 12 min", "AAC 48 kHz stereo", "2 readers: 1 RTSP, 1 RTMP", "12.3 MB sent"]
                    .into_iter()
                    .map(String::from)
                    .collect::<Vec<_>>()
            )
        );
        assert_eq!(streams[1].1, ["Not arriving: its audio is not reaching MediaMTX", "No readers", "2 broken frames"]);
        let (down, facts, _) = relay_card(&relay(RelayState::Down, vec![]));
        assert_eq!(down, "Not answering. Is it running? `docker compose ps` lists it.");
        assert_eq!(facts, ["RTSP localhost:8555", "SRT localhost:8891", "RTMP localhost:1936"], "nothing it said before is shown as current");
    }

    #[test]
    fn data_sizes_read_short() {
        assert_eq!(data(900), "900 B");
        assert_eq!(data(45_000), "45 KB");
        assert_eq!(data(12_300_000), "12.3 MB");
        assert_eq!(data(2_500_000_000), "2.5 GB");
    }

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
        assert_eq!(playlist_mark(&[]), "", "no name when in none");
        assert_eq!(playlist_mark(&[p("Rainy jazz")]), "Rainy jazz");
        assert_eq!(playlist_mark(&[p("A"), p("B"), p("C")]), "A +2", "the first name, and how many more");
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
        assert_eq!(
            engine_card(&down),
            (
                "ace-step".into(),
                "Not answering. Check that its server is running and reachable at the address in the configuration file.".into(),
                vec![]
            )
        );
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
