//! The web app reads the API through `ui::api`'s types; these tests feed them the real server's JSON,
//! so a field renamed on one side fails here, not in a browser.

mod common;

use common::{browser, script, send, sign_in, world};
use maio::{
    domain::{Playlist, job::Job},
    ui::api::{Failure, PlaylistSummary, PlaylistView, SongPage, SongView, Status},
};
use serde_json::{Value, json};

fn fits<T: serde::de::DeserializeOwned>(v: Value) -> T {
    serde_json::from_value(v.clone()).unwrap_or_else(|e| panic!("{e}: {v}"))
}

#[tokio::test]
async fn songs_playlists_status_and_jobs_fit() {
    let w = world();
    let song = w.songs_with_one(); // see Step 11
    let (_, _, page) = send(&w, script("GET", "/api/v1/songs", None)).await;
    let page: SongPage = fits(page);
    assert_eq!(page.songs[0].song.id, song);

    let (_, _, p) = send(&w, script("POST", "/api/v1/playlists", Some(json!({"name": "Rainy jazz"})))).await;
    let p: Playlist = fits(p);
    let (_, _, added) = send(&w, script("POST", &format!("/api/v1/playlists/{}/songs", p.id), Some(json!({"song_ids": [song]})))).await;
    let _: Playlist = fits(added);
    let (_, _, list) = send(&w, script("GET", "/api/v1/playlists", None)).await;
    let list: Vec<PlaylistSummary> = fits(list);
    assert_eq!(list[0].songs, 1);
    let (_, _, view) = send(&w, script("GET", &format!("/api/v1/playlists/{}", p.id), None)).await;
    let view: PlaylistView = fits(view);
    assert_eq!(view.songs.len(), 1);
    let (_, _, renamed) = send(&w, script("PATCH", &format!("/api/v1/songs/{song}"), Some(json!({"title": "Low Tide"})))).await;
    let renamed: SongView = fits(renamed);
    assert_eq!(renamed.playlists[0].name, "Rainy jazz");

    let (_, _, status) = send(&w, script("GET", "/api/v1/status", None)).await;
    let status: Status = fits(status);
    assert_eq!(status.moods, ["calm"]);
    let (_, _, accepted) = send(&w, script("POST", "/api/v1/songs", Some(json!({"prompt": "pads"})))).await;
    let _: maio::ui::api::Accepted = fits(accepted);
    let (_, _, jobs) = send(&w, script("GET", "/api/v1/jobs", None)).await;
    let _: Vec<Job> = fits(jobs);
}

#[tokio::test]
async fn errors_fit_failure() {
    let w = world();
    let song = w.songs_with_one();
    let (_, _, p) = send(&w, script("POST", "/api/v1/playlists", Some(json!({"name": "Rainy jazz"})))).await;
    send(&w, script("POST", &format!("/api/v1/playlists/{}/songs", p["id"].as_str().unwrap()), Some(json!({"song_ids": [song]})))).await;
    let (status, _, body) = send(&w, script("DELETE", &format!("/api/v1/songs/{song}"), None)).await;
    let f = Failure::parse(status.as_u16(), &body.to_string(), None);
    assert_eq!(f.code, "song_in_playlist");
    assert_eq!(f.text(), "Remove it from Rainy jazz first.");
}

#[tokio::test]
async fn search_with_reserved_characters() {
    let w = world();
    let session = sign_in(&w).await;
    let q = maio::ui::api::enc("rain & jazz #2");
    let (status, _, body) = send(&w, browser("GET", &format!("/api/v1/songs?q={q}&limit=50"), Some(&session), None)).await;
    assert_eq!(status, 200, "{body}");
    let _: SongPage = fits(body);
}

#[tokio::test(flavor = "multi_thread")]
async fn live_fits() {
    let w = world();
    let song = common::tone_song(&w, "s1");
    let (_, _, p) = send(&w, script("POST", "/api/v1/playlists", Some(json!({"name": "Late"})))).await;
    let id = p["id"].as_str().unwrap().to_string();
    send(&w, script("POST", &format!("/api/v1/playlists/{id}/songs"), Some(json!({"song_ids": [song]})))).await;
    let (_, _, live) = send(&w, script("PUT", &format!("/api/v1/playlists/{id}/live"), Some(json!({"live": true})))).await;
    let live: maio::ui::api::LiveJson = fits(live);
    assert!(live.live);
    assert!(live.addresses.unwrap().mp3.unwrap().ends_with("/stream.mp3"));
    let (_, _, off) = send(&w, script("GET", &format!("/api/v1/playlists/{id}/live"), None)).await;
    let _: maio::ui::api::LiveJson = fits(off);
}
