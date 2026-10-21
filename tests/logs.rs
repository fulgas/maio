//! What the request logs say (arc42 8.6), alone in its own binary: tracing caches whether a log line is wanted,
//! and tests running beside this one without a subscriber would make it cache "no".

mod common;

use axum::http::{Request, StatusCode};
use common::{script, send, tone_song, world};
use serde_json::json;
use tower::ServiceExt;
/// Collects what the logs would say, for checks on what they must not.
#[derive(Clone, Default)]
struct Logs(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for Logs {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn private_listen_keys_stay_out_of_the_logs() {
    let w = world();
    let song = tone_song(&w, "s1");
    let (_, _, p) = send(&w, script("POST", "/api/v1/playlists", Some(json!({"name": "Late"})))).await;
    let id = p["id"].as_str().unwrap().to_string();
    send(&w, script("POST", &format!("/api/v1/playlists/{id}/songs"), Some(json!({"song_ids": [song]})))).await;
    let (_, _, live) = send(&w, script("PUT", &format!("/api/v1/playlists/{id}/live"), Some(json!({"live": true, "visibility": "private"})))).await;
    let mp3 = live["addresses"]["mp3"].as_str().unwrap().to_string();
    let key = mp3.split('/').nth(2).unwrap().to_string();
    let logs = Logs::default();
    let sink = logs.clone();
    let subscriber = tracing_subscriber::fmt().with_writer(move || sink.clone()).with_ansi(false).finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let res = w.router.clone().oneshot(Request::get(&mp3).body(axum::body::Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    drop(res);
    let text = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
    assert!(text.contains("/listen/***/stream.mp3"), "the request is logged: {text}");
    assert!(!text.contains(&key), "the key is not: {text}");
}
