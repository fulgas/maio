mod common;

use maio::usecases::ports::{PlaylistRepository, SongRepository};
use axum::http::StatusCode;
use common::*;
use maio::domain::testing::song;
use serde_json::{Value, json};

fn ids(view: &Value) -> Vec<String> {
    view["playlist"]["song_ids"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect()
}

#[tokio::test]
async fn a_playlist_is_built_ordered_and_trimmed() {
    let w = world();
    for id in ["a", "b", "c"] {
        w.songs.insert(&song(id)).unwrap();
    }
    let mut events = w.app.events.subscribe();
    let (status, _, late) = send(&w, script("POST", "/api/v1/playlists", Some(json!({"name": "Late"})))).await;
    assert_eq!((status, late["name"].as_str()), (StatusCode::CREATED, Some("Late")));
    let base = format!("/api/v1/playlists/{}", late["id"].as_str().unwrap());

    send(&w, script("POST", &format!("{base}/songs"), Some(json!({"song_ids": ["a", "b", "a"]})))).await;
    let (status, _, _) = send(&w, script("POST", &format!("{base}/songs"), Some(json!({"song_ids": ["c"], "position": 0})))).await;
    assert_eq!(status, StatusCode::OK);
    let (_, _, view) = send(&w, script("GET", &base, None)).await;
    assert_eq!(ids(&view), ["c", "a", "b"]);
    assert_eq!(view["songs"].as_array().unwrap().len(), 3);

    let (status, _, body) = send(&w, script("PUT", &format!("{base}/songs"), Some(json!({"song_ids": ["a", "a", "b"]})))).await;
    assert_eq!((status, body["error"]["code"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some("bad_order")));
    send(&w, script("PUT", &format!("{base}/songs"), Some(json!({"song_ids": ["b", "c", "a"]})))).await;
    send(&w, script("DELETE", &format!("{base}/songs/c"), None)).await;
    let (_, _, view) = send(&w, script("GET", &base, None)).await;
    assert_eq!(ids(&view), ["b", "a"]);

    let (status, _, renamed) = send(&w, script("PATCH", &base, Some(json!({"name": "Later"})))).await;
    assert_eq!((status, renamed["name"].as_str()), (StatusCode::OK, Some("Later")));
    let (_, _, list) = send(&w, script("GET", "/api/v1/playlists", None)).await;
    assert_eq!((list[0]["name"].as_str(), list[0]["songs"].clone(), list[0]["seconds"].clone()), (Some("Later"), json!(2), json!(360.0)));

    let mut kinds = Vec::new();
    while let Ok(e) = events.try_recv() {
        kinds.push(e.kind);
    }
    assert!(kinds.len() >= 6 && kinds.iter().all(|k| *k == "playlist"), "{kinds:?}");
}

#[tokio::test]
async fn missing_things_and_live_playlists_are_refused() {
    let w = world();
    w.songs.insert(&song("a")).unwrap();
    let (status, _, body) = send(&w, script("GET", "/api/v1/playlists/nope", None)).await;
    assert_eq!((status, body["error"]["code"].as_str()), (StatusCode::NOT_FOUND, Some("not_found")));
    let (_, _, late) = send(&w, script("POST", "/api/v1/playlists", Some(json!({"name": "Late"})))).await;
    let id = late["id"].as_str().unwrap().to_string();
    let base = format!("/api/v1/playlists/{id}");
    assert_eq!(send(&w, script("POST", &format!("{base}/songs"), Some(json!({"song_ids": ["ghost"]})))).await.0, StatusCode::NOT_FOUND);
    assert_eq!(send(&w, script("DELETE", &format!("{base}/songs/a"), None)).await.0, StatusCode::NOT_FOUND, "a is not in it");
    let (status, _, body) = send(&w, script("POST", "/api/v1/playlists", Some(json!({"name": "  "})))).await;
    assert_eq!((status, body["error"]["code"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some("bad_name")));

    let mut stored = w.playlists.get(&id).unwrap().unwrap();
    stored.live.on = true;
    w.playlists.save(&stored).unwrap();
    let (status, _, body) = send(&w, script("DELETE", &base, None)).await;
    assert_eq!((status, body["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("playlist_live")));
    stored.live.on = false;
    w.playlists.save(&stored).unwrap();
    assert_eq!(send(&w, script("DELETE", &base, None)).await.0, StatusCode::NO_CONTENT);
    assert_eq!(send(&w, script("GET", &base, None)).await.0, StatusCode::NOT_FOUND);
}
