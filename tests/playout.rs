mod common;

use std::collections::HashSet;

use common::{Outcome, on_air, playlist, sample_track, spawn_fake_engine, test_state};
use maio::{
    jobs,
    playlists::{Kind, Order, Playlist, Recipe},
    playout::{self, LATE_S},
    queue::{Entry, EntryState, Position, Source},
    request::Normalized,
    state::AppState,
};

fn generative() -> Playlist {
    let recipe: Recipe = serde_json::from_str(r#"{"mood":"calm","seconds":[15,20],"library_share":0}"#).unwrap();
    Playlist { name: "late night".into(), recipe: Some(recipe), ..playlist("st1", Kind::Generative, Order::Shuffle) }
}

/// A fixed playlist of `ids`, each a track in the crate.
fn fixed(state: &AppState, id: &str, order: Order, ids: &[&str]) -> Playlist {
    for t in ids {
        if state.store.track(t).unwrap().is_none() {
            state.store.insert_track(&sample_track(t, None, None, "/x.flac")).unwrap();
        }
    }
    let p = playlist(id, Kind::Fixed, order);
    state.store.insert_playlist(&p).unwrap();
    state.store.add_to_playlist(id, &ids.iter().map(|s| s.to_string()).collect::<Vec<_>>(), None).unwrap();
    p
}

/// Plays `n` tracks from `p` on main, recording each in history; returns their ids.
fn play(state: &AppState, p: &Playlist, n: usize) -> Vec<String> {
    (0..n).map(|_| {
        let t = playout::next_track(state, &state.main(), p).unwrap();
        state.store.play_started(&t.id, &t.title, "main").unwrap();
        t.id
    }).collect()
}

#[tokio::test]
async fn with_nothing_on_air_and_nothing_queued_fill_adds_nothing() {
    let (state, _dir) = test_state(vec![], "").await;
    for id in ["a", "b", "c"] {
        state.store.insert_track(&sample_track(id, None, None, "/x.flac")).unwrap();
    }
    playout::fill(&state, &state.main());
    assert_eq!(state.main().queue.lock().unwrap().pending_count(), 0, "idle means silence");
    assert!(playout::pick_next(&state, &state.main(), 0.0).is_none(), "an empty queue does not pull from the crate");
}

#[tokio::test]
async fn a_generative_playlist_generates_with_a_healthy_engine() {
    let fake = spawn_fake_engine("fake", "yes", Outcome::Hang).await;
    let (state, _dir) = test_state(vec![(fake, "fake")], "").await;
    on_air(&state, "main", &generative());
    playout::fill(&state, &state.main());
    let main = state.main();
    let q = main.queue.lock().unwrap();
    let up: Vec<_> = q.upcoming().collect();
    assert_eq!(up.len(), 2);
    assert!(up.iter().all(|e| e.source == Source::Playlist && e.state == EntryState::Generating && e.filler));
    assert_eq!(up[0].title, "late night");
}

#[tokio::test]
async fn a_generative_playlist_replays_its_own_tracks_when_engines_are_down() {
    let (state, _dir) = test_state(vec![], "").await;
    on_air(&state, "main", &generative());
    state.store.insert_track(&sample_track("other", None, None, "/x.flac")).unwrap();
    state.store.insert_track(&sample_track("mine", None, Some("st1"), "/x.flac")).unwrap();
    state.store.add_to_playlist("st1", &["mine".into()], None).unwrap();
    playout::fill(&state, &state.main());
    let main = state.main();
    let q = main.queue.lock().unwrap();
    let first = q.upcoming().next().unwrap();
    assert_eq!((first.track_id.as_deref(), first.source), (Some("mine"), Source::Playlist));
}

#[tokio::test]
async fn a_generative_playlist_at_its_size_only_replays() {
    let fake = spawn_fake_engine("fake", "yes", Outcome::Hang).await;
    let (state, _dir) = test_state(vec![(fake, "fake")], "").await;
    let p = Playlist { max_tracks: Some(2), ..generative() };
    on_air(&state, "main", &p);
    for id in ["m1", "m2"] {
        state.store.insert_track(&sample_track(id, Some("calm"), Some("st1"), "/x.flac")).unwrap();
    }
    state.store.add_to_playlist("st1", &["m1".into(), "m2".into()], None).unwrap();
    playout::fill(&state, &state.main());
    assert!(state.jobs.lock().unwrap().is_empty(), "no new tracks at max_tracks");
    let ids: HashSet<String> = state.main().queue.lock().unwrap().upcoming().filter_map(|e| e.track_id.clone()).collect();
    assert_eq!(ids, HashSet::from(["m1".to_string(), "m2".to_string()]));
}

#[tokio::test]
async fn fill_with_nothing_available_adds_nothing() {
    let (state, _dir) = test_state(vec![], "").await;
    on_air(&state, "main", &generative());
    playout::fill(&state, &state.main());
    assert_eq!(state.main().queue.lock().unwrap().pending_count(), 0);
}

#[tokio::test]
async fn a_fixed_playlist_in_order_loops_in_its_order() {
    let (state, _dir) = test_state(vec![], "").await;
    let p = fixed(&state, "pl_f", Order::InOrder, &["a", "b", "c"]);
    assert_eq!(play(&state, &p, 7), ["a", "b", "c", "a", "b", "c", "a"]);
}

#[tokio::test]
async fn a_shuffled_playlist_never_repeats_within_no_repeat() {
    let (state, _dir) = test_state(vec![], "no_repeat = 2").await;
    let p = fixed(&state, "pl_s", Order::Shuffle, &["a", "b", "c", "d"]);
    let got = play(&state, &p, 40);
    for w in got.windows(3) {
        assert!(w[2] != w[0] && w[2] != w[1], "{w:?} repeats within 2 plays");
    }
    assert_eq!(got.iter().collect::<HashSet<_>>().len(), 4, "every track gets played");
}

#[tokio::test]
async fn a_one_track_playlist_loops_that_track() {
    let (state, _dir) = test_state(vec![], "").await;
    let p = fixed(&state, "pl_1", Order::InOrder, &["only"]);
    assert_eq!(play(&state, &p, 3), ["only", "only", "only"]);
}

#[tokio::test]
async fn fill_queues_side_b_from_a_fixed_playlist_never_twice_at_once() {
    let (state, _dir) = test_state(vec![], "").await;
    let p = fixed(&state, "pl_f", Order::InOrder, &["a", "b"]);
    on_air(&state, "main", &p);
    playout::fill(&state, &state.main());
    let main = state.main();
    let q = main.queue.lock().unwrap();
    let up: Vec<_> = q.upcoming().map(|e| (e.track_id.clone().unwrap(), e.filler, e.source)).collect();
    assert_eq!(up, [("a".to_string(), true, Source::Playlist), ("b".to_string(), true, Source::Playlist)]);
}

#[tokio::test]
async fn smart_and_builtin_playlists_play_only_their_tracks() {
    let (state, _dir) = test_state(vec![], "").await;
    state.store.insert_track(&sample_track("calm", Some("calm"), None, "/x.flac")).unwrap();
    state.store.insert_track(&sample_track("tense", Some("tense"), None, "/x.flac")).unwrap();
    let smart = Playlist { rule: Some(maio::playlists::Rule { mood: Some("tense".into()), ..Default::default() }), ..playlist("pl_s", Kind::Smart, Order::Shuffle) };
    assert_eq!(play(&state, &smart, 3), ["tense", "tense", "tense"]);
    state.store.set_favourite("calm", true).unwrap();
    assert_eq!(play(&state, &Playlist::builtin("kept").unwrap(), 2), ["calm", "calm"]);
}

#[tokio::test]
async fn a_deleted_track_in_the_deck_is_never_queued() {
    let (state, _dir) = test_state(vec![], "").await;
    let p = fixed(&state, "pl_f", Order::InOrder, &["a", "b", "c"]);
    assert_eq!(play(&state, &p, 1), ["a"]);
    state.store.mark_deleted("b").unwrap();
    assert_eq!(play(&state, &p, 3), ["c", "a", "c"]);
}

#[tokio::test]
async fn an_on_air_playlist_that_no_longer_exists_means_silence() {
    let (state, _dir) = test_state(vec![], "").await;
    state.store.insert_track(&sample_track("a", None, None, "/x.flac")).unwrap();
    state.store.set_on_air("main", Some("st_gone")).unwrap();
    playout::fill(&state, &state.main());
    assert_eq!(state.main().queue.lock().unwrap().pending_count(), 0);
}

#[tokio::test]
async fn two_radios_with_one_playlist_keep_their_own_decks() {
    let (state, _dir) = test_state(vec![], "").await;
    let lobby = maio::radio::Radio::new(maio::store::RadioRow {
        id: "lobby".into(), name: "Lobby".into(), visibility: maio::store::Visibility::Public, listen_key: "k".repeat(32), created_at: maio::store::now(),
    });
    state.radios.lock().unwrap().push(lobby.clone());
    let p = fixed(&state, "pl_f", Order::InOrder, &["a", "b"]);
    on_air(&state, "main", &p);
    on_air(&state, "lobby", &p);
    playout::fill(&state, &state.main());
    playout::fill(&state, &lobby);
    let first = |r: &maio::radio::Radio| r.queue.lock().unwrap().upcoming().next().unwrap().track_id.clone().unwrap();
    assert_eq!((first(&state.main()), first(&lobby)), ("a".to_string(), "a".to_string()), "each radio starts the playlist on its own");
}

#[tokio::test]
async fn late_fallback_matches_mood_over_the_playlist_on_air() {
    let (state, _dir) = test_state(vec![], "").await;
    on_air(&state, "main", &generative());
    state.store.insert_track(&sample_track("station_calm", Some("calm"), Some("st1"), "/x.flac")).unwrap();
    state.store.add_to_playlist("st1", &["station_calm".into()], None).unwrap();
    state.store.insert_track(&sample_track("tense", Some("tense"), None, "/x.flac")).unwrap();
    let n = Normalized {
        prompt: "p".into(), seconds: 30, bpm: None, vocals: false, lyrics: None, seed: 1, engine: None,
        allow_noncommercial: false, mood: Some("tense".into()), genre: None, energy: None, title: "tense".into(), vocal_language: "en".into(), lyrics_by: None, user_prompt: None,
    };
    state.main().queue.lock().unwrap().add(Entry::generating(jobs::new_id("q"), jobs::new_id("job"), &n, Source::Manual, "user:root", None, None), Position::End);
    let (_, track) = playout::pick_next(&state, &state.main(), 1.0).unwrap();
    assert_eq!(track.id, "tense", "SPEC 5.5: same mood as the late entry");
}

#[tokio::test]
async fn a_generative_playlist_backs_off_after_failed_jobs() {
    let bad = spawn_fake_engine("bad", "yes", Outcome::Fail { retryable: false }).await;
    let (state, _dir) = test_state(vec![(bad, "bad")], "").await;
    on_air(&state, "main", &generative());
    playout::fill(&state, &state.main());
    assert_eq!(state.jobs.lock().unwrap().len(), 2);
    for _ in 0..100 {
        if state.main().queue.lock().unwrap().pending_count() == 0 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(state.main().queue.lock().unwrap().pending_count(), 0, "both jobs failed");
    playout::fill(&state, &state.main());
    assert_eq!(state.jobs.lock().unwrap().len(), 2, "no new jobs while backing off");
}

#[tokio::test]
async fn pick_next_on_empty_library_is_none() {
    let (state, _dir) = test_state(vec![], "").await;
    assert!(playout::pick_next(&state, &state.main(), 0.0).is_none());
}

fn generating_entry(state: &maio::state::AppState) -> String {
    let n = Normalized {
        prompt: "p".into(), seconds: 30, bpm: None, vocals: false, lyrics: None, seed: 1, engine: None,
        allow_noncommercial: false, mood: Some("calm".into()), genre: None, energy: None, title: "calm".into(), vocal_language: "en".into(), lyrics_by: None, user_prompt: None,
    };
    let id = jobs::new_id("q");
    state.main().queue.lock().unwrap().add(Entry::generating(id.clone(), jobs::new_id("job"), &n, Source::Manual, "me", None, None), Position::End);
    id
}

#[tokio::test]
async fn pick_next_waits_for_a_generating_entry_while_there_is_time() {
    let (state, _dir) = test_state(vec![], "").await;
    state.store.insert_track(&sample_track("calm", Some("calm"), None, "/x.flac")).unwrap();
    generating_entry(&state);
    assert!(playout::pick_next(&state, &state.main(), LATE_S + 10.0).is_none());
}

#[tokio::test]
async fn pick_next_falls_back_to_the_library_when_late() {
    let (state, _dir) = test_state(vec![], "").await;
    state.store.insert_track(&sample_track("other", Some("tense"), None, "/x.flac")).unwrap();
    state.store.insert_track(&sample_track("calm", Some("calm"), None, "/x.flac")).unwrap();
    let waiting = generating_entry(&state);
    let (entry_id, track) = playout::pick_next(&state, &state.main(), 1.0).unwrap();
    assert_eq!(track.id, "calm", "same mood as the late entry");
    let main = state.main();
    let q = main.queue.lock().unwrap();
    let order: Vec<_> = q.upcoming().map(|e| e.id.clone()).collect();
    assert_eq!(order, [entry_id, waiting], "fallback plays first, the late entry stays next");
}

#[tokio::test]
async fn ready_entry_with_a_deleted_track_fails_and_is_skipped() {
    let (state, _dir) = test_state(vec![], "").await;
    let t = sample_track("gone", None, None, "/x.flac");
    let id = jobs::new_id("q");
    state.main().queue.lock().unwrap().add(Entry::ready(id.clone(), &t, Source::Manual, "me"), Position::End); // not in the store
    assert!(playout::pick_next(&state, &state.main(), 0.0).is_none());
    assert_eq!(state.main().queue.lock().unwrap().get_mut(&id).unwrap().state, EntryState::Failed);
}

#[tokio::test]
async fn missing_file_fails_the_entry_on_decode() {
    let (state, _dir) = test_state(vec![], "").await;
    state.store.insert_track(&sample_track("nofile", None, None, "/nonexistent/nofile.flac")).unwrap();
    let t = state.store.track("nofile").unwrap().unwrap();
    let id = jobs::new_id("q");
    state.main().queue.lock().unwrap().add(Entry::ready(id.clone(), &t, Source::Manual, "me"), Position::End);
    let rx = playout::load_next(&state, &state.main(), 0.0).unwrap();
    assert!(rx.await.unwrap().is_none());
    assert_eq!(state.main().queue.lock().unwrap().get_mut(&id).unwrap().state, EntryState::Failed);
}

#[tokio::test]
async fn real_file_decodes_into_a_deck() {
    let (state, dir) = test_state(vec![], "").await;
    let file = dir.path().join("tone.flac");
    let wav = dir.path().join("tone.wav");
    maio::media::test_tone(&wav, 1).await.unwrap();
    maio::media::import(&wav, &file, &Default::default()).await.unwrap();
    let t = sample_track("tone", None, None, file.to_str().unwrap());
    state.store.insert_track(&t).unwrap();
    state.main().queue.lock().unwrap().add(Entry::ready(jobs::new_id("q"), &t, Source::Manual, "me"), Position::End);
    let deck = playout::load_next(&state, &state.main(), 0.0).unwrap().await.unwrap().unwrap();
    assert_eq!(deck.track_id, "tone");
    assert!(deck.samples.len() > 90_000);
}

#[tokio::test]
async fn missing_file_retires_the_track_so_it_is_not_picked_again() {
    let (state, _dir) = test_state(vec![], "").await;
    let t = sample_track("nofile", None, None, "/nonexistent/nofile.flac");
    state.store.insert_track(&t).unwrap();
    state.main().queue.lock().unwrap().add(Entry::ready(jobs::new_id("q"), &t, Source::Manual, "me"), Position::End);
    let rx = playout::load_next(&state, &state.main(), 0.0).unwrap();
    assert!(rx.await.unwrap().is_none());
    assert!(state.store.track("nofile").unwrap().is_none(), "track retired");
    assert!(playout::pick_next(&state, &state.main(), 0.0).is_none(), "nothing left to pick");
}

async fn tone_track(state: &maio::state::AppState, dir: &std::path::Path, id: &str, freq: u32) -> maio::store::Track {
    let wav = dir.join(format!("{id}.wav"));
    let ok = tokio::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-v", "error", "-y", "-f", "lavfi", "-i"])
        .arg(format!("sine=frequency={freq}:duration=20"))
        .arg(&wav)
        .status().await.unwrap().success();
    assert!(ok);
    let flac = dir.join(format!("{id}.flac"));
    maio::media::import(&wav, &flac, &Default::default()).await.unwrap();
    let t = sample_track(id, None, None, flac.to_str().unwrap());
    state.store.insert_track(&t).unwrap();
    t
}

fn rms(chunk: &[u8]) -> f64 {
    let n = chunk.len() / 2;
    let sum: f64 = chunk.chunks_exact(2).map(|b| f64::from(i16::from_le_bytes([b[0], b[1]])).powi(2)).sum();
    (sum / n as f64).sqrt()
}

#[tokio::test]
async fn skip_crossfades_without_a_silent_gap() {
    let (state, dir) = test_state(vec![], "").await; // crossfade_s = 0.5
    let a = tone_track(&state, dir.path(), "a", 440).await;
    let b = tone_track(&state, dir.path(), "b", 660).await;
    for t in [&a, &b] {
        state.main().queue.lock().unwrap().add(Entry::ready(jobs::new_id("q"), t, Source::Manual, "me"), Position::End);
    }
    let mut pcm = state.main().pcm.subscribe();
    tokio::spawn(playout::run(state.clone(), state.main()));
    // let track a play for about 1 s
    let mut heard = 0;
    while heard < 50 {
        let c = pcm.recv().await.unwrap();
        if rms(&c) > 500.0 {
            heard += 1;
        }
    }
    let first = state.main().playout.lock().unwrap().now.as_ref().unwrap().track.id.clone();
    assert_eq!(first, "a");
    state.main().playout.lock().unwrap().skip = true;
    let mut quietest = f64::MAX;
    for _ in 0..150 {
        quietest = quietest.min(rms(&pcm.recv().await.unwrap()));
    }
    let now = state.main().playout.lock().unwrap().now.as_ref().unwrap().track.id.clone();
    assert_eq!(now, "b", "skip moved to the next track");
    assert!(quietest > 500.0, "a 20 ms chunk went quiet during the skip (rms {quietest:.0})");
}

#[tokio::test]
async fn a_closed_radio_stops_its_playout_loop() {
    let (state, _dir) = test_state(vec![], "").await;
    let r = state.main();
    let task = tokio::spawn(maio::playout::run(state.clone(), r.clone()));
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    r.closed.send_replace(true);
    tokio::time::timeout(std::time::Duration::from_secs(2), task).await.expect("the loop ends").unwrap();
}
