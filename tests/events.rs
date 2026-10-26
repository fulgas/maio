//! Every change reaches the browsers as an event that carries the new state, so the web app never fetches to catch
//! up (arc42 3.5, ADR 0018).

mod common;

use common::*;
use serde_json::{Value, json};
use tokio::sync::broadcast::Receiver;

use maio::web::events::Event;

fn drain(rx: &mut Receiver<Event>) -> Vec<(&'static str, Value)> {
    let mut all = Vec::new();
    while let Ok(e) = rx.try_recv() {
        all.push((e.kind, e.data));
    }
    all
}

fn find<'a>(events: &'a [(&'static str, Value)], kind: &str, id: &str) -> &'a Value {
    &events.iter().rev().find(|(k, d)| *k == kind && d["id"] == id).unwrap_or_else(|| panic!("no {kind} event for {id} in {events:?}")).1
}

#[tokio::test]
async fn a_renamed_song_arrives_with_its_new_title() {
    let w = world();
    let id = w.songs_with_one();
    let mut rx = w.app.events.subscribe();
    send(&w, script("PATCH", &format!("/api/v1/songs/{id}"), Some(json!({"title": "Night Bus"})))).await;
    let events = drain(&mut rx);
    let e = find(&events, "song", &id);
    assert_eq!((e["deleted"].clone(), e["song"]["title"].clone()), (json!(false), json!("Night Bus")));
}

#[tokio::test]
async fn a_deleted_song_arrives_as_deleted() {
    let w = world();
    let id = w.songs_with_one();
    let mut rx = w.app.events.subscribe();
    send(&w, script("DELETE", &format!("/api/v1/songs/{id}"), None)).await;
    let events = drain(&mut rx);
    let e = find(&events, "song", &id);
    assert_eq!((e["deleted"].clone(), e["song"].clone()), (json!(true), Value::Null));
}

#[tokio::test]
async fn a_playlist_change_carries_the_playlist_and_the_songs_whose_marks_changed() {
    let w = world();
    let song = w.songs_with_one();
    let (_, _, made) = send(&w, script("POST", "/api/v1/playlists", Some(json!({"name": "Late"})))).await;
    let pid = made["id"].as_str().unwrap().to_string();
    let mut rx = w.app.events.subscribe();

    send(&w, script("POST", &format!("/api/v1/playlists/{pid}/songs"), Some(json!({"song_ids": [song]})))).await;
    let events = drain(&mut rx);
    let p = find(&events, "playlist", &pid);
    assert_eq!((p["summary"]["songs"].clone(), p["view"]["songs"][0]["id"].clone()), (json!(1), json!(song)), "its summary and its songs");
    assert_eq!(find(&events, "song", &song)["song"]["playlists"][0]["name"], "Late", "the song now carries the playlist's mark");

    send(&w, script("PATCH", &format!("/api/v1/playlists/{pid}"), Some(json!({"name": "Later"})))).await;
    let events = drain(&mut rx);
    assert_eq!(find(&events, "playlist", &pid)["summary"]["name"], "Later");
    assert_eq!(find(&events, "song", &song)["song"]["playlists"][0]["name"], "Later", "its songs' marks are renamed too");

    send(&w, script("DELETE", &format!("/api/v1/playlists/{pid}/songs/{song}"), None)).await;
    let events = drain(&mut rx);
    assert_eq!(find(&events, "song", &song)["song"]["playlists"], json!([]), "a removed song loses the mark");

    send(&w, script("POST", &format!("/api/v1/playlists/{pid}/songs"), Some(json!({"song_ids": [song]})))).await;
    drain(&mut rx);
    send(&w, script("DELETE", &format!("/api/v1/playlists/{pid}"), None)).await;
    let events = drain(&mut rx);
    assert_eq!(find(&events, "playlist", &pid)["deleted"], true);
    assert_eq!(find(&events, "song", &song)["song"]["playlists"], json!([]), "a deleted playlist's songs lose the mark");
}

#[tokio::test]
async fn going_live_carries_the_live_state_and_the_playlist() {
    let w = world();
    let (_, _, made) = send(&w, script("POST", "/api/v1/playlists", Some(json!({"name": "Air"})))).await;
    let pid = made["id"].as_str().unwrap().to_string();
    let mut rx = w.app.events.subscribe();
    send(&w, script("PUT", &format!("/api/v1/playlists/{pid}/live"), Some(json!({"live": true})))).await;
    let events = drain(&mut rx);
    assert_eq!(find(&events, "live", &pid)["live"]["live"], true, "the live state, as GET .../live answers");
    assert_eq!(find(&events, "playlist", &pid)["summary"]["live"]["on"], true, "and the playlist's live mark");
}

#[tokio::test]
async fn going_live_tells_every_live_playlist_how_many_streams_are_in_use() {
    let w = world();
    let mut ids = vec![];
    for name in ["Air", "Late"] {
        let (_, _, made) = send(&w, script("POST", "/api/v1/playlists", Some(json!({"name": name})))).await;
        ids.push(made["id"].as_str().unwrap().to_string());
    }
    send(&w, script("PUT", &format!("/api/v1/playlists/{}/live", ids[0]), Some(json!({"live": true})))).await;
    let mut rx = w.app.events.subscribe();
    send(&w, script("PUT", &format!("/api/v1/playlists/{}/live", ids[1]), Some(json!({"live": true})))).await;
    let events = drain(&mut rx);
    assert_eq!(find(&events, "live", &ids[0])["live"]["live_count"], 2, "the other live strip shows the new count");
}
