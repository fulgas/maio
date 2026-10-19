//! A prompt becomes a tagged song in the library: the real engine client, ffmpeg and SQLite together.

use std::{path::Path, process::Command, sync::Arc, time::Duration};

use axum::{
    Json, Router,
    routing::{get, post},
};
use maio::adapters::{
    engine::{EngineConfig, HttpEngine},
    ffmpeg::{Ffmpeg, StoreFormat},
    sqlite::{Db, SqliteSongs},
    system::{RandomIds, SystemClock},
};
use maio::domain::{job::JobState, limits::Rate, request::SongRequest};
use maio::usecases::{
    MakeSettings, Making, MakingPorts,
    ports::{MusicEngine, SongRepository},
    testing::MemNotifier,
};
use serde_json::json;

fn tone(path: &Path, secs: u32) {
    let ok = Command::new("ffmpeg")
        .args(["-hide_banner", "-v", "error", "-y", "-f", "lavfi", "-i"])
        .arg(format!("sine=frequency=440:duration={secs}"))
        .arg(path)
        .status()
        .unwrap()
        .success();
    assert!(ok, "ffmpeg makes a test tone");
}

/// A fake `acestep-api` that finishes every song at once and hands back `wav`.
async fn serve_engine(wav: Vec<u8>) -> String {
    let wrap = |data: serde_json::Value| Json(json!({"data": data, "code": 200, "error": null}));
    let app = Router::new()
        .route(
            "/health",
            get(move || async move {
                wrap(json!({"status": "ok", "service": "ACE-Step API", "version": "1.0", "models_initialized": true,
                    "loaded_model": "acestep-v15-turbo", "loaded_lm_model": null}))
            }),
        )
        .route("/release_task", post(move || async move { wrap(json!({"task_id": "t1", "status": "queued"})) }))
        .route(
            "/query_result",
            post(move || async move {
                let result = json!([{"file": "/v1/audio?path=t1", "lyrics": "[Instrumental]", "seed_value": "3",
                    "metas": {"duration": 20.0, "bpm": 70}}]);
                wrap(json!([{"task_id": "t1", "status": 1, "result": result.to_string()}]))
            }),
        )
        .route(
            "/v1/audio",
            get(move || {
                let wav = wav.clone();
                async move { wav }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    url
}

#[tokio::test]
async fn a_prompt_becomes_a_tagged_opus_song_in_the_library() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("tone.wav");
    tone(&wav, 20);
    let engine = Arc::new(HttpEngine::new(EngineConfig { id: "ace-step".into(), url: serve_engine(std::fs::read(&wav).unwrap()).await }));
    engine.check().await;
    let songs = Arc::new(SqliteSongs(Arc::new(Db::open(&dir.path().join("library.db")).unwrap())));
    let making = Making::new(
        MakingPorts {
            engines: vec![engine as Arc<dyn MusicEngine>],
            text_engines: vec![],
            songs: songs.clone(),
            files: Arc::new(Ffmpeg::new(dir.path().join("data"), StoreFormat::Opus, 160).unwrap()),
            ids: Arc::new(RandomIds),
            clock: Arc::new(SystemClock),
            notifier: Arc::new(MemNotifier::default()),
        },
        MakeSettings {
            moods: vec!["calm".into()],
            genres: vec![],
            default_language: "en".into(),
            default_lyrics_by: "ace-step".into(),
            naming_engine: None,
            make_rate: Rate { max: 20, window_s: 600 },
            pending_jobs: 4,
            poll: Duration::from_millis(10),
            lyrics_timeout: Duration::from_secs(1),
            naming_timeout: Duration::from_secs(1),
            stall: Duration::from_secs(60),
        },
    );

    let accepted =
        making.make(&SongRequest { prompt: Some("warm pads".into()), mood: Some("calm".into()), seconds: Some(20), ..Default::default() }).unwrap();
    let mut job = making.job(&accepted.job_id).unwrap();
    for _ in 0..1500 {
        if !job.state.is_open() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
        job = making.job(&accepted.job_id).unwrap();
    }
    assert_eq!(job.state, JobState::Done, "{job:?}");

    let song = songs.get(job.song_id.as_deref().unwrap()).unwrap().unwrap();
    let file = dir.path().join("data").join(&song.file);
    assert!(song.file.ends_with(".opus") && file.exists(), "{}", song.file);
    assert!((song.seconds - 20.0).abs() < 0.2, "{}", song.seconds);
    assert_eq!((song.seed, song.prompt.as_str()), (3, "warm pads"));
    let probe = Command::new("ffprobe").args(["-v", "error", "-show_entries", "stream_tags=title", "-of", "csv=p=0"]).arg(&file).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&probe.stdout).trim(), song.title, "the file carries the title");
    assert!(std::fs::read_dir(dir.path().join("data/tmp")).unwrap().next().is_none(), "the engine's WAV is cleaned up");
}
