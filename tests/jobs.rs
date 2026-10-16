mod common;

use std::time::Duration;

use common::{Outcome, spawn_fake_engine, test_state};
use maio::{
    jobs::{self, JobState},
    queue::{Entry, EntryState, Position, Source},
    request::Normalized,
    state::AppState,
};

fn request() -> Normalized {
    Normalized {
        prompt: "calm mood, instrumental".into(), seconds: 3, bpm: None, vocals: false, lyrics: None, seed: 1,
        engine: None, allow_noncommercial: false, mood: Some("calm".into()), genre: None, energy: None, title: "calm".into(), vocal_language: "en".into(), lyrics_by: None, user_prompt: None,
    }
}

/// Queues a generating entry and starts its job; returns (entry_id, job_id).
fn start(state: &AppState) -> (String, String) {
    let (entry_id, job_id) = (jobs::new_id("q"), jobs::new_id("job"));
    let n = request();
    let entry = Entry::generating(entry_id.clone(), job_id.clone(), &n, Source::Manual, "me", None, None);
    state.main().queue.lock().unwrap().add(entry, Position::End);
    jobs::start(state, job_id.clone(), n, "me", Some("main"), Some(entry_id.clone()), None);
    (entry_id, job_id)
}

async fn wait_job(state: &AppState, job_id: &str, want: JobState) -> jobs::JobStatus {
    for _ in 0..200 {
        let j = state.jobs.lock().unwrap().get(job_id).cloned().unwrap();
        if j.state == want {
            return j;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("job {job_id} never reached {want:?}");
}

fn entry_state(state: &AppState, id: &str) -> EntryState {
    state.main().queue.lock().unwrap().get_mut(id).unwrap().state
}

#[tokio::test]
async fn finished_job_imports_a_track_and_readies_the_entry() {
    let fake = spawn_fake_engine("fake", "limited", Outcome::Done).await;
    let (state, _dir) = test_state(vec![(fake, "fake")], "").await;
    let (entry_id, job_id) = start(&state);
    let job = wait_job(&state, &job_id, JobState::Done).await;
    let track = state.store.track(job.track_id.as_deref().unwrap()).unwrap().unwrap();
    assert!(std::path::Path::new(&track.file).exists());
    assert!((track.seconds - 3.0).abs() < 0.1);
    assert_eq!((track.engine.as_str(), track.commercial.as_str()), ("fake", "limited"));
    assert!(job.warning.unwrap().contains("limited test licence"));
    assert_eq!(entry_state(&state, &entry_id), EntryState::Ready);
    assert_eq!((track.title_source.as_str(), track.artist.as_str(), track.album.as_str()), ("words", "Maio", "Requests"));
    assert_ne!(track.title, "calm", "named from word lists, not the mood");
    assert_eq!(state.main().queue.lock().unwrap().get_mut(&entry_id).unwrap().title, track.title, "the queue shows the real title");
    let tags = maio::media::read_tags(std::path::Path::new(&track.file)).await.unwrap();
    assert_eq!((tags.get("title"), tags.get("artist")), (Some(&track.title), Some(&"Maio".to_string())));
    assert_eq!(track.bytes as u64, std::fs::metadata(&track.file).unwrap().len(), "size measured after tagging");
}

#[tokio::test]
async fn retryable_failure_moves_to_the_next_engine() {
    let bad = spawn_fake_engine("bad", "yes", Outcome::Fail { retryable: true }).await;
    let good = spawn_fake_engine("good", "yes", Outcome::Done).await;
    let (state, _dir) = test_state(vec![(bad, "bad"), (good, "good")], "").await;
    let (_, job_id) = start(&state);
    let job = wait_job(&state, &job_id, JobState::Done).await;
    assert_eq!(state.store.track(&job.track_id.unwrap()).unwrap().unwrap().engine, "good");
}

#[tokio::test]
async fn vanished_job_is_retried_elsewhere() {
    let gone = spawn_fake_engine("gone", "yes", Outcome::Vanish).await;
    let good = spawn_fake_engine("good", "yes", Outcome::Done).await;
    let (state, _dir) = test_state(vec![(gone, "gone"), (good, "good")], "").await;
    let (_, job_id) = start(&state);
    wait_job(&state, &job_id, JobState::Done).await;
}

#[tokio::test]
async fn non_retryable_failure_fails_the_entry() {
    let bad = spawn_fake_engine("bad", "yes", Outcome::Fail { retryable: false }).await;
    let (state, _dir) = test_state(vec![(bad, "bad")], "").await;
    let (entry_id, job_id) = start(&state);
    let job = wait_job(&state, &job_id, JobState::Failed).await;
    assert!(job.error.unwrap().contains("scripted failure"));
    assert_eq!(entry_state(&state, &entry_id), EntryState::Failed);
}

#[tokio::test]
async fn cancelled_job_frees_the_engine_job() {
    let fake = spawn_fake_engine("slow", "yes", Outcome::Hang).await;
    let deleted = fake.deleted.clone();
    let (state, _dir) = test_state(vec![(fake, "slow")], "").await;
    let (_, job_id) = start(&state);
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(jobs::pending_for(&state, "me"), 1);
    jobs::cancel(&state, &[job_id.clone()]);
    assert_eq!(jobs::pending_for(&state, "me"), 0);
    let job = wait_job(&state, &job_id, JobState::Failed).await;
    assert_eq!(job.error.as_deref(), Some("cancelled"));
    assert_eq!(deleted.lock().unwrap().as_slice(), ["j1"]);
}

#[tokio::test]
async fn no_engine_fails_at_once() {
    let (state, _dir) = test_state(vec![], "").await;
    let (entry_id, job_id) = start(&state);
    wait_job(&state, &job_id, JobState::Failed).await;
    assert_eq!(entry_state(&state, &entry_id), EntryState::Failed);
}

#[tokio::test]
async fn transport_failures_mark_the_engine_down() {
    use maio::engine::{Caps, Commercial, Engine, EngineInfo, Health, Licence};
    let info = EngineInfo {
        protocol: 1, id: "dead".into(), name: "dead".into(), version: "1".into(),
        licence: Licence { name: "X".into(), commercial: Commercial::Yes, url: String::new(), note: String::new() },
        capabilities: Caps { min_seconds: 1, max_seconds: 600, vocals: false, tempo: true, concurrency: 1 },
        realtime_factor: 0.1,
    };
    let cfg = maio::config::EngineConfig { id: "dead".into(), url: "http://127.0.0.1:9".into(), token_env: "X".into(), token: "t".into() };
    let engine = std::sync::Arc::new(Engine::with_state(cfg, Some(info), Health::Up));
    let (state, _dir) = test_state(vec![], "").await;
    let state = maio::state::AppState::with_engines(state.cfg.clone(), maio::store::Store::open_in_memory().unwrap(), vec![engine.clone()]).unwrap();
    for _ in 0..3 {
        let (_, job_id) = start(&state);
        wait_job(&state, &job_id, JobState::Failed).await;
    }
    assert_eq!(engine.snapshot().1, Health::Down);
}

#[tokio::test]
async fn a_track_made_for_a_generative_playlist_joins_it() {
    let fake = spawn_fake_engine("fake", "yes", Outcome::Done).await;
    let (state, _dir) = test_state(vec![(fake, "fake")], "").await;
    let p = common::playlist("pl_g", maio::playlists::Kind::Generative, maio::playlists::Order::Shuffle);
    state.store.insert_playlist(&p).unwrap();
    let job_id = jobs::new_id("job");
    jobs::start(&state, job_id.clone(), request(), "playlist", Some("main"), None, Some("pl_g".into()));
    let job = wait_job(&state, &job_id, JobState::Done).await;
    let track = job.track_id.unwrap();
    assert_eq!(state.store.track(&track).unwrap().unwrap().made_by.as_deref(), Some("pl_g"));
    assert_eq!(state.store.member_ids(&p, None).unwrap(), [track.clone()]);
    let t = state.store.track(&track).unwrap().unwrap();
    assert_eq!((t.artist.as_str(), t.album.as_str()), ("name pl_g", "name pl_g"), "artist and album are the playlist");
}

fn sung() -> Normalized {
    Normalized { vocals: true, prompt: "rainy bus".into(), user_prompt: Some("rainy bus".into()), ..request() }
}

async fn run_job(state: &AppState, n: Normalized) -> (maio::store::Track, jobs::JobStatus) {
    let job_id = jobs::new_id("job");
    jobs::start(state, job_id.clone(), n, "user:ana", Some("main"), None, None);
    let job = wait_job(state, &job_id, JobState::Done).await;
    (state.store.track(job.track_id.as_deref().unwrap()).unwrap().unwrap(), job)
}

#[tokio::test]
async fn ace_step_writes_the_lyrics_when_no_text_engine_is_set() {
    let fake = common::spawn_fake_engine_with("ace-step", "yes", Outcome::Done, true).await;
    let requests = fake.requests.clone();
    let (state, _dir) = test_state(vec![(fake, "ace-step")], "").await;
    let (t, _) = run_job(&state, sung()).await;
    let sent = requests.lock().unwrap()[0].clone();
    assert_eq!((sent["write_lyrics"].as_bool(), sent["vocal_language"].as_str()), (Some(true), Some("en")));
    assert_eq!((t.lyrics_engine.as_deref(), t.key.as_deref(), t.time_signature.as_deref()), (Some("ace-step"), Some("C major"), Some("4")));
    assert_eq!((t.title.as_str(), t.title_source.as_str()), ("Engine Sang This", "lyrics"));
    assert_eq!((t.vocal_language.as_deref(), t.bpm), (Some("en"), Some(92)));
}

#[tokio::test]
async fn a_text_engine_writes_lyrics_and_ace_step_sings_them() {
    let text = common::spawn_fake_text(common::FakeReply::Text("[verse]\nwet windows\n[chorus]\nride the night bus")).await;
    let fake = common::spawn_fake_engine_with("ace-step", "yes", Outcome::Done, true).await;
    let requests = fake.requests.clone();
    let toml = format!("{}\n[lyrics]\ndefault_engine = \"local\"\n", common::text_engine_toml("local", "openai", &text.url));
    let (state, _dir) = test_state(vec![(fake, "ace-step")], &toml).await;
    let (t, _) = run_job(&state, sung()).await;
    let sent = requests.lock().unwrap()[0].clone();
    assert_eq!((sent.get("write_lyrics"), sent["lyrics"].as_str()), (None, Some("[verse]\nwet windows\n[chorus]\nride the night bus")), "write_lyrics false is not sent");
    assert_eq!((t.lyrics_engine.as_deref(), t.title.as_str()), (Some("local"), "Ride The Night Bus"));
    let asked = text.seen.lock().unwrap()[0].2.to_string();
    assert!(asked.contains("rainy bus") && !asked.contains("instrumental, rainy"), "only the person's prompt and fields go out: {asked}");
}

#[tokio::test]
async fn a_failing_text_engine_falls_back_to_ace_step_and_says_so() {
    let text = common::spawn_fake_text(common::FakeReply::Status(500)).await;
    let fake = common::spawn_fake_engine_with("ace-step", "yes", Outcome::Done, true).await;
    let requests = fake.requests.clone();
    let toml = format!("{}\n[lyrics]\ndefault_engine = \"local\"\n", common::text_engine_toml("local", "openai", &text.url));
    let (state, _dir) = test_state(vec![(fake, "ace-step")], &toml).await;
    let (t, job) = run_job(&state, sung()).await;
    assert_eq!(requests.lock().unwrap()[0]["write_lyrics"].as_bool(), Some(true));
    assert_eq!(t.lyrics_engine.as_deref(), Some("ace-step"));
    assert!(job.note.unwrap().contains("local"), "the job notes the fallback");
}

#[tokio::test]
async fn empty_lyrics_from_a_text_engine_fall_back_to_ace_step() {
    let text = common::spawn_fake_text(common::FakeReply::Text("   ")).await;
    let fake = common::spawn_fake_engine_with("ace-step", "yes", Outcome::Done, true).await;
    let requests = fake.requests.clone();
    let toml = format!("{}\n[lyrics]\ndefault_engine = \"local\"\n", common::text_engine_toml("local", "openai", &text.url));
    let (state, _dir) = test_state(vec![(fake, "ace-step")], &toml).await;
    run_job(&state, sung()).await;
    assert_eq!(requests.lock().unwrap()[0]["write_lyrics"].as_bool(), Some(true));
}

#[tokio::test]
async fn a_naming_engine_names_the_song_and_a_failing_one_does_not_block() {
    let text = common::spawn_fake_text(common::FakeReply::Text(r#"{"title": "Night Bus", "description": "A slow ride home."}"#)).await;
    let fake = spawn_fake_engine("fake", "yes", Outcome::Done).await;
    let toml = format!("{}\n[naming]\nengine = \"local\"\n", common::text_engine_toml("local", "openai", &text.url));
    let (state, _dir) = test_state(vec![(fake, "fake")], &toml).await;
    let (t, _) = run_job(&state, request()).await;
    assert_eq!((t.title.as_str(), t.title_source.as_str(), t.title_engine.as_deref(), t.description.as_deref()),
               ("Night Bus", "engine", Some("local"), Some("A slow ride home.")));
    let down = common::spawn_fake_text(common::FakeReply::Status(503)).await;
    let fake = spawn_fake_engine("fake", "yes", Outcome::Done).await;
    let toml = format!("{}\n[naming]\nengine = \"local\"\n", common::text_engine_toml("local", "openai", &down.url));
    let (state, _dir) = test_state(vec![(fake, "fake")], &toml).await;
    let (t, _) = run_job(&state, request()).await;
    assert_eq!((t.title_source.as_str(), t.title_engine), ("words", None));
}
