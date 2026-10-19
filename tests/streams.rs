//! Live over HTTP (SPEC 9, 10): the live API, the streams, and the relay's auth hook.

mod common;

use axum::http::{Request, StatusCode, header};
use common::{script, send, tone_song, world};
use serde_json::json;
use tower::ServiceExt;

async fn go_live(w: &common::World, visibility: &str) -> (String, serde_json::Value) {
    let song = tone_song(w, "s1");
    let (_, _, p) = send(w, script("POST", "/api/v1/playlists", Some(json!({"name": "Late"})))).await;
    let id = p["id"].as_str().unwrap().to_string();
    send(w, script("POST", &format!("/api/v1/playlists/{id}/songs"), Some(json!({"song_ids": [song]})))).await;
    let (status, _, live) =
        send(w, script("PUT", &format!("/api/v1/playlists/{id}/live"), Some(json!({"live": true, "visibility": visibility})))).await;
    assert_eq!(status, StatusCode::OK, "{live}");
    (id, live)
}

async fn get(w: &common::World, path: &str, icy: bool) -> axum::response::Response {
    let mut r = Request::get(path);
    if icy {
        r = r.header("icy-metadata", "1");
    }
    w.router.clone().oneshot(r.body(axum::body::Body::empty()).unwrap()).await.unwrap()
}

async fn first_chunk(res: axum::response::Response) -> Vec<u8> {
    use http_body_util::BodyExt;
    let mut body = res.into_body();
    let frame = tokio::time::timeout(std::time::Duration::from_secs(10), body.frame()).await.expect("bytes in time").unwrap().unwrap();
    frame.into_data().unwrap().to_vec()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_live_playlist_streams_mp3_at_its_public_address() {
    let w = world();
    let (id, live) = go_live(&w, "public").await;
    assert_eq!(live["addresses"]["mp3"], format!("/live/{id}/stream.mp3"));
    let res = get(&w, &format!("/live/{id}/stream.mp3"), false).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()[header::CONTENT_TYPE], "audio/mpeg");
    assert!(!first_chunk(res).await.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn icy_listeners_get_titles() {
    let w = world();
    let (id, _) = go_live(&w, "public").await;
    let res = get(&w, &format!("/live/{id}/stream.mp3"), true).await;
    assert_eq!(res.headers()["icy-metaint"], "16000");
}

#[tokio::test(flavor = "multi_thread")]
async fn going_private_ends_public_streams() {
    let w = world();
    let (id, _) = go_live(&w, "public").await;
    let res = get(&w, &format!("/live/{id}/stream.mp3"), false).await;
    let (_, _, live) = send(&w, script("PUT", &format!("/api/v1/playlists/{id}/live"), Some(json!({"live": true, "visibility": "private"})))).await;
    use http_body_util::BodyExt;
    let collected = tokio::time::timeout(std::time::Duration::from_secs(5), res.into_body().collect()).await;
    assert!(collected.is_ok(), "the open public stream ended");
    assert_eq!(get(&w, &format!("/live/{id}/stream.mp3"), false).await.status(), StatusCode::NOT_FOUND);
    let private = live["addresses"]["mp3"].as_str().unwrap();
    assert!(private.starts_with("/listen/"));
    assert_eq!(get(&w, private, false).await.status(), StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread")]
async fn listener_count_follows_connections() {
    let w = world();
    let (id, _) = go_live(&w, "public").await;
    let a = get(&w, &format!("/live/{id}/stream.mp3"), false).await;
    let b = get(&w, &format!("/live/{id}/stream.aac"), false).await;
    let (_, _, live) = send(&w, script("GET", &format!("/api/v1/playlists/{id}/live"), None)).await;
    assert_eq!(live["on_air"]["listeners"], 2);
    drop((a, b));
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let (_, _, live) = send(&w, script("GET", &format!("/api/v1/playlists/{id}/live"), None)).await;
    assert_eq!(live["on_air"]["listeners"], 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_live_limit_is_a_409_naming_the_live_ones() {
    let w = world(); // max_live = 2
    for name in ["A", "B"] {
        let (_, _, p) = send(&w, script("POST", "/api/v1/playlists", Some(json!({"name": name})))).await;
        send(&w, script("PUT", &format!("/api/v1/playlists/{}/live", p["id"].as_str().unwrap()), Some(json!({"live": true})))).await;
    }
    let (_, _, c) = send(&w, script("POST", "/api/v1/playlists", Some(json!({"name": "C"})))).await;
    let (status, _, body) =
        send(&w, script("PUT", &format!("/api/v1/playlists/{}/live", c["id"].as_str().unwrap()), Some(json!({"live": true})))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "live_limit");
    assert_eq!(body["error"]["playlists"].as_array().unwrap().len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_live_playlist_cannot_be_deleted_and_off_air_stops_it() {
    let w = world();
    let (id, _) = go_live(&w, "public").await;
    let (status, _, _) = send(&w, script("DELETE", &format!("/api/v1/playlists/{id}"), None)).await;
    assert_eq!(status, StatusCode::CONFLICT);
    send(&w, script("PUT", &format!("/api/v1/playlists/{id}/live"), Some(json!({"live": false})))).await;
    assert_eq!(get(&w, &format!("/live/{id}/stream.mp3"), false).await.status(), StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread")]
async fn streams_need_no_session_but_the_live_api_does() {
    let w = world();
    let (id, _) = go_live(&w, "public").await;
    assert_eq!(get(&w, &format!("/live/{id}/stream.aac"), false).await.status(), StatusCode::OK);
    let r = Request::get(format!("/api/v1/playlists/{id}/live")).body(axum::body::Body::empty()).unwrap();
    assert_eq!(w.router.clone().oneshot(r).await.unwrap().status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_relay_hook_lets_anyone_read_a_live_path_only() {
    let w = world();
    let (id, _) = go_live(&w, "public").await;
    let ask = |body: serde_json::Value| {
        Request::post("/relay/auth").header(header::CONTENT_TYPE, "application/json").body(axum::body::Body::from(body.to_string())).unwrap()
    };
    let ok = w.router.clone().oneshot(ask(json!({"action": "read", "path": format!("live/{id}")}))).await.unwrap();
    assert_eq!(ok.status(), StatusCode::OK);
    let no = w.router.clone().oneshot(ask(json!({"action": "read", "path": "live/nope"}))).await.unwrap();
    assert_eq!(no.status(), StatusCode::UNAUTHORIZED);
}
