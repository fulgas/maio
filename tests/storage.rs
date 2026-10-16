mod common;

use axum::{body::Body, http::{Request, StatusCode}};
use common::{Outcome, sample_track, spawn_fake_engine, test_state};
use maio::{api, playlists::{Kind, Order, Playlist}, playout, storage};
use serde_json::{Value, json};
use tower::ServiceExt;

/// 3000 bytes of budget, so a few 1000-byte sample tracks fill it.
const TINY: &str = "storage = { max_gb = 0.000003 }";

#[tokio::test]
async fn making_room_deletes_least_recently_played_unkept_tracks() {
    let (state, dir) = test_state(vec![], TINY).await;
    for id in ["a", "b", "kept"] {
        let file = dir.path().join(format!("{id}.opus"));
        std::fs::write(&file, b"x").unwrap();
        state.store.insert_track(&sample_track(id, None, None, file.to_str().unwrap())).unwrap();
    }
    state.store.set_favourite("kept", true).unwrap();
    assert!(storage::make_room(&state, 1_000, &[]), "deleting one track makes room");
    assert_eq!(state.store.usage().unwrap(), 2_000);
    assert!(state.store.track("a").unwrap().is_none(), "the oldest unkept track went first");
    assert!(!dir.path().join("a.opus").exists());
    assert!(!storage::make_room(&state, 5_000, &[]), "kept tracks are never deleted, so it cannot fit");
    assert!(state.store.track("kept").unwrap().is_some());
}

async fn call(app: &axum::Router, method: &str, path: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(path).header("host", "localhost:8080").header("origin", "http://localhost:8080").header("cookie", "maio_session=sess-root");
    let req = match body {
        Some(b) => { req = req.header("content-type", "application/json"); req.body(Body::from(b.to_string())).unwrap() }
        None => req.body(Body::empty()).unwrap(),
    };
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

#[tokio::test]
async fn a_crate_full_of_kept_tracks_refuses_new_music_but_plays_old() {
    let fake = spawn_fake_engine("fake", "yes", Outcome::Hang).await;
    let (state, _dir) = test_state(vec![(fake, "fake")], TINY).await;
    common::person(&state, "root", "admin");
    for id in ["k1", "k2", "k3"] {
        state.store.insert_track(&sample_track(id, Some("calm"), None, "/nonexistent/x.opus")).unwrap();
        state.store.set_favourite(id, true).unwrap();
    }
    let app = api::router(state.clone());
    let (s, body) = call(&app, "POST", "/api/v1/queue", Some(json!({"request": {"mood": "calm", "seconds": 60}}))).await;
    assert_eq!((s, body["error"]["code"].as_str()), (StatusCode::INSUFFICIENT_STORAGE, Some("library_full")));
    assert_eq!(call(&app, "POST", "/api/v1/queue", Some(json!({"track_id": "k1"}))).await.0, StatusCode::ACCEPTED);
    let (_, status) = call(&app, "GET", "/api/v1/status", None).await;
    assert_eq!(status["storage"]["full"], true);
    assert_eq!(status["storage"]["format"], "opus");
    assert_eq!(status["storage"]["used_bytes"], 3_000);
}

#[tokio::test]
async fn a_generative_playlist_on_a_full_crate_loops_its_own_tracks_instead_of_generating() {
    let fake = spawn_fake_engine("fake", "yes", Outcome::Hang).await;
    let (state, _dir) = test_state(vec![(fake, "fake")], TINY).await;
    common::person(&state, "root", "admin");
    let p = Playlist {
        recipe: Some(serde_json::from_str(r#"{"mood":"calm","seconds":[20,30],"library_share":0}"#).unwrap()),
        ..common::playlist("st1", Kind::Generative, Order::Shuffle)
    };
    common::on_air(&state, "main", &p);
    for id in ["s1", "s2", "s3"] {
        state.store.insert_track(&sample_track(id, Some("calm"), Some("st1"), "/nonexistent/x.opus")).unwrap();
        state.store.set_favourite(id, true).unwrap();
        state.store.add_to_playlist("st1", &[id.to_string()], None).unwrap();
    }
    playout::fill(&state, &state.main());
    assert!(state.jobs.lock().unwrap().is_empty(), "no generation while full");
    assert_eq!(state.main().queue.lock().unwrap().pending_count(), 2, "the playlist replays its own tracks");
}
