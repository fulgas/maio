mod common;

use std::{sync::atomic::Ordering, time::Duration};

use axum::http::{StatusCode, header};
use common::*;
use maio::domain::{Song, engine::Health, testing::song};
use maio::usecases::ports::SongRepository;
use serde_json::{Value, json};
use tower::ServiceExt;

/// Waits until nothing is waiting or making, at most 2 s.
async fn until_no_jobs(w: &World, session: &str) {
    for _ in 0..400 {
        if send(w, browser("GET", "/api/v1/jobs", Some(session), None)).await.2 == json!([]) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("jobs never finished");
}

async fn make_song(w: &World, session: &str) -> Value {
    let (status, _, body) = send(w, browser("POST", "/api/v1/songs", Some(session), Some(json!({"prompt": "warm pads", "mood": "calm"})))).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{body}");
    until_no_jobs(w, session).await;
    send(w, browser("GET", "/api/v1/songs", Some(session), None)).await.2["songs"][0].clone()
}

#[tokio::test]
async fn a_prompt_becomes_a_song_and_events_tell_about_it() {
    let w = world();
    let s = sign_in(&w).await;
    let mut events = w.app.events.subscribe();
    let (status, _, accepted) = send(&w, browser("POST", "/api/v1/songs", Some(&s), Some(json!({"prompt": "warm pads", "mood": "calm"})))).await;
    assert_eq!((status, accepted["eta_s"].clone()), (StatusCode::ACCEPTED, json!(120)));
    until_no_jobs(&w, &s).await;
    let (status, _, page) = send(&w, browser("GET", "/api/v1/songs", Some(&s), None)).await;
    assert_eq!(status, StatusCode::OK);
    let song = &page["songs"][0];
    assert_eq!((song["prompt"].as_str(), song["playlists"].clone(), page["next"].clone()), (Some("warm pads"), json!([]), Value::Null));
    let mut kinds = Vec::new();
    while let Ok(e) = events.try_recv() {
        kinds.push(e.kind);
    }
    assert!(kinds.contains(&"job") && kinds.contains(&"song"), "{kinds:?}");
}

#[tokio::test]
async fn events_stream_as_server_sent_events() {
    let w = world();
    let s = sign_in(&w).await;
    let res = w.router.clone().oneshot(browser("GET", "/api/v1/events", Some(&s), None)).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()[header::CONTENT_TYPE], "text/event-stream");
}

#[tokio::test]
async fn bad_requests_are_refused_with_the_rule_that_failed() {
    let w = world();
    let s = sign_in(&w).await;
    let code = |body: Value| body["error"]["code"].as_str().unwrap_or_default().to_string();
    let (status, _, body) = send(&w, browser("POST", "/api/v1/songs", Some(&s), Some(json!({})))).await;
    assert_eq!((status, code(body)), (StatusCode::UNPROCESSABLE_ENTITY, "missing_mood_or_prompt".into()));
    let (status, _, body) = send(&w, browser("POST", "/api/v1/songs", Some(&s), Some(json!({"mood": "calm", "bogus": 1})))).await;
    assert_eq!((status, code(body)), (StatusCode::UNPROCESSABLE_ENTITY, "invalid_body".into()));
    w.engine.status.lock().unwrap().health = Health::Down;
    let (status, _, body) = send(&w, browser("POST", "/api/v1/songs", Some(&s), Some(json!({"mood": "calm"})))).await;
    assert_eq!((status, code(body)), (StatusCode::SERVICE_UNAVAILABLE, "engine_unavailable".into()));
    let (status, _, body) = send(&w, browser("GET", "/api/v1/songs?sort=loud", Some(&s), None)).await;
    assert_eq!((status, code(body)), (StatusCode::UNPROCESSABLE_ENTITY, "bad_sort".into()));
    let (status, _, body) = send(&w, browser("GET", "/api/v1/songs?limit=lots", Some(&s), None)).await;
    assert_eq!((status, code(body)), (StatusCode::UNPROCESSABLE_ENTITY, "invalid_query".into()));
    let (status, _, body) = send(&w, browser("GET", "/api/v1/songs/nope", Some(&s), None)).await;
    assert_eq!((status, code(body)), (StatusCode::NOT_FOUND, "not_found".into()));
}

#[tokio::test]
async fn the_make_rate_answers_429_with_retry_after() {
    let w = world();
    for _ in 0..3 {
        assert_eq!(send(&w, script("POST", "/api/v1/songs", Some(json!({"mood": "calm"})))).await.0, StatusCode::ACCEPTED);
    }
    let (status, headers, body) = send(&w, script("POST", "/api/v1/songs", Some(json!({"mood": "calm"})))).await;
    assert_eq!((status, body["error"]["code"].as_str()), (StatusCode::TOO_MANY_REQUESTS, Some("rate_limited")));
    assert_eq!(headers[header::RETRY_AFTER], "600");
}

#[tokio::test]
async fn a_job_can_be_cancelled_once() {
    let w = world();
    w.engine.hold.store(true, Ordering::SeqCst);
    let (_, _, accepted) = send(&w, script("POST", "/api/v1/songs", Some(json!({"mood": "calm"})))).await;
    let path = format!("/api/v1/jobs/{}", accepted["job_id"].as_str().unwrap());
    let (status, _, job) = send(&w, script("DELETE", &path, None)).await;
    assert_eq!((status, job["state"].as_str()), (StatusCode::OK, Some("cancelled")));
    let (status, _, body) = send(&w, script("DELETE", &path, None)).await;
    assert_eq!((status, body["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("job_finished")));
    assert_eq!(send(&w, script("GET", "/api/v1/jobs", None)).await.2, json!([]));
}

#[tokio::test]
async fn renaming_and_deleting_a_song() {
    let w = world();
    let s = sign_in(&w).await;
    let id = make_song(&w, &s).await["id"].as_str().unwrap().to_string();
    let path = format!("/api/v1/songs/{id}");
    let (status, _, renamed) = send(&w, browser("PATCH", &path, Some(&s), Some(json!({"title": " Night Bus "})))).await;
    assert_eq!((status, renamed["title"].as_str(), renamed["title_source"].as_str()), (StatusCode::OK, Some("Night Bus"), Some("person")));
    assert!(w.files.retagged.lock().unwrap().contains(&"Night Bus".to_string()));

    let late = w.app.playlists.create("Late").unwrap();
    w.app.playlists.add_songs(&late.id, std::slice::from_ref(&id), None).unwrap();
    let (status, _, body) = send(&w, browser("DELETE", &path, Some(&s), None)).await;
    assert_eq!((status, body["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("song_in_playlist")));
    assert_eq!(body["error"]["playlists"], json!([{"id": late.id, "name": "Late"}]));

    w.app.playlists.remove_song(&late.id, &id).unwrap();
    assert_eq!(send(&w, browser("DELETE", &path, Some(&s), None)).await.0, StatusCode::NO_CONTENT);
    assert_eq!(*w.files.deleted.lock().unwrap(), [format!("songs/{id}.opus")]);
    assert_eq!(send(&w, browser("GET", &path, Some(&s), None)).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn song_audio_answers_range_requests() {
    let w = world();
    w.songs.insert(&Song { file: "songs/s1.opus".into(), ..song("s1") }).unwrap();
    std::fs::create_dir_all(w.dir.path().join("songs")).unwrap();
    std::fs::write(w.dir.path().join("songs/s1.opus"), b"OggS and the rest of the file").unwrap();
    let mut req = script("GET", "/api/v1/songs/s1/audio", None);
    req.headers_mut().insert(header::RANGE, "bytes=0-3".parse().unwrap());
    let res = w.router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(&axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap()[..], b"OggS");
}

#[tokio::test]
async fn an_open_event_stream_ends_when_the_server_stops() {
    let w = world();
    let s = sign_in(&w).await;
    let res = w.router.clone().oneshot(browser("GET", "/api/v1/events", Some(&s), None)).await.unwrap();
    w.app.stop();
    let ended = tokio::time::timeout(Duration::from_secs(2), axum::body::to_bytes(res.into_body(), usize::MAX)).await;
    assert!(ended.is_ok(), "the stream must end, or a graceful shutdown waits for ever");
}
