mod common;

use common::{Outcome, engine_config, spawn_fake_engine};
use maio::engine::{CallError, Engine, EngineRequest, Health};

fn request() -> EngineRequest {
    EngineRequest { prompt: "calm".into(), seconds: 3, bpm: None, vocals: false, lyrics: None, seed: 1, vocal_language: "en".into(), write_lyrics: false }
}

#[tokio::test]
async fn talks_to_an_engine_server() {
    let fake = spawn_fake_engine("fake", "yes", Outcome::Done).await;
    let engine = Engine::new(engine_config(&fake, "fake"));
    engine.check().await;
    let (info, health) = engine.snapshot();
    assert_eq!(health, Health::Up);
    assert_eq!(info.unwrap().id, "fake");

    let job = engine.submit(&request()).await.unwrap();
    assert_eq!(engine.job(&job).await.unwrap().state, "done");
    let dir = tempfile::tempdir().unwrap();
    let dst = dir.path().join("a.wav");
    engine.download(&job, &dst).await.unwrap();
    assert!(std::fs::metadata(&dst).unwrap().len() > 1000);
    engine.delete(&job).await;
    assert_eq!(fake.deleted.lock().unwrap().as_slice(), ["j1"]);
}

#[tokio::test]
async fn wrong_token_marks_the_engine_down_after_three_checks() {
    let fake = spawn_fake_engine("fake", "yes", Outcome::Done).await;
    let mut cfg = engine_config(&fake, "fake");
    cfg.token = "wrong".into();
    let engine = Engine::new(cfg);
    for _ in 0..2 {
        engine.check().await;
    }
    assert_ne!(engine.snapshot().1, Health::Down);
    engine.check().await;
    assert_eq!(engine.snapshot().1, Health::Down);
}

#[tokio::test]
async fn vanished_job_is_not_found() {
    let fake = spawn_fake_engine("fake", "yes", Outcome::Vanish).await;
    let engine = Engine::new(engine_config(&fake, "fake"));
    assert!(matches!(engine.job("j1").await, Err(CallError::NotFound)));
}
