mod common;

use axum::{Router, body::Body, http::{Request, StatusCode}};
use common::{SITE, sample_track, test_state};
use maio::{api, state::AppState};
use serde_json::{Value, json};
use tower::ServiceExt;

const OP: &str = "sess-ana";
const LIVE: &str = "sess-guest";

async fn app() -> (Router, AppState, tempfile::TempDir) {
    let (state, dir) = test_state(vec![], "playlists = { max_tracks = 3, max_tracks_limit = 5 }").await;
    common::person(&state, "ana", "operator");
    common::person(&state, "guest", "listener");
    (api::router(state.clone()), state, dir)
}

async fn call(app: &Router, method: &str, path: &str, token: &str, body: Option<Value>) -> (StatusCode, Value) {
    let req = Request::builder().method(method).uri(path).header("host", "localhost:8080").header("origin", SITE)
        .header("cookie", format!("maio_session={token}"));
    let req = match body {
        Some(b) => req.header("content-type", "application/json").body(Body::from(b.to_string())).unwrap(),
        None => req.body(Body::empty()).unwrap(),
    };
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

fn ids(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap().to_string()).collect()
}

fn tracks(state: &AppState, list: &[&str]) {
    for id in list {
        state.store.insert_track(&sample_track(id, Some("calm"), None, "/x.flac")).unwrap();
    }
}

#[tokio::test]
async fn builtins_come_first_and_cannot_be_changed() {
    let (app, _state, _d) = app().await;
    call(&app, "POST", "/api/v1/playlists", OP, Some(json!({"name": "Mine", "kind": "fixed"}))).await;
    let (s, list) = call(&app, "GET", "/api/v1/playlists", LIVE, None).await;
    assert_eq!(s, StatusCode::OK);
    let got: Vec<&str> = list["playlists"].as_array().unwrap().iter().map(|p| p["id"].as_str().unwrap()).take(3).collect();
    assert_eq!(got, ["crate", "kept", "requests"]);
    assert_eq!(list["playlists"][3]["name"], "Mine");
    assert_eq!(list["playlists"][0]["max_tracks"], Value::Null, "built-ins have no limit");
    assert_eq!(call(&app, "PATCH", "/api/v1/playlists/kept", OP, Some(json!({"name": "x"}))).await.1["error"]["code"], "builtin_playlist");
    assert_eq!(call(&app, "DELETE", "/api/v1/playlists/crate", OP, None).await.1["error"]["code"], "builtin_playlist");
    assert_eq!(call(&app, "POST", "/api/v1/playlists", OP, Some(json!({"name": "x", "kind": "crate"}))).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(call(&app, "POST", "/api/v1/playlists/requests/tracks", OP, Some(json!({"track_ids": []}))).await.1["error"]["code"], "playlist_not_editable");
}

#[tokio::test]
async fn a_fixed_playlist_is_filled_reordered_emptied_and_has_a_size() {
    let (app, state, _d) = app().await;
    tracks(&state, &["a", "b", "c", "d"]);
    let (s, p) = call(&app, "POST", "/api/v1/playlists", OP, Some(json!({"name": "Mine", "kind": "fixed"}))).await;
    assert_eq!((s, p["order"].as_str(), p["max_tracks"].as_u64(), p["count"].as_u64()), (StatusCode::CREATED, Some("in_order"), Some(3), Some(0)));
    let id = p["id"].as_str().unwrap().to_string();
    let tracks_url = format!("/api/v1/playlists/{id}/tracks");
    assert_eq!(call(&app, "POST", &tracks_url, OP, Some(json!({"track_ids": ["a", "b"]}))).await.0, StatusCode::OK);
    assert_eq!(call(&app, "POST", &tracks_url, OP, Some(json!({"track_ids": ["c"], "position": 0}))).await.1["count"], 3);
    let (s, full) = call(&app, "POST", &tracks_url, OP, Some(json!({"track_ids": ["d"]}))).await;
    assert_eq!((s, full["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("playlist_full")));
    assert_eq!(call(&app, "POST", &tracks_url, OP, Some(json!({"track_ids": ["a"]}))).await.0, StatusCode::OK, "already in it: nothing added, not full");
    assert_eq!(call(&app, "POST", &tracks_url, OP, Some(json!({"track_ids": ["zzz"]}))).await.1["error"]["code"], "unknown_track");
    let (_, got) = call(&app, "GET", &format!("/api/v1/playlists/{id}"), LIVE, None).await;
    assert_eq!(ids(&got["tracks"]), ["c", "a", "b"]);
    assert_eq!(call(&app, "PUT", &tracks_url, OP, Some(json!({"track_ids": ["b", "a"]}))).await.0, StatusCode::OK);
    assert_eq!(call(&app, "PUT", &tracks_url, OP, Some(json!({"track_ids": ["b", "b"]}))).await.1["error"]["code"], "duplicate_track");
    assert_eq!(call(&app, "DELETE", &format!("{tracks_url}/b"), OP, None).await.0, StatusCode::NO_CONTENT);
    assert_eq!(call(&app, "DELETE", &format!("{tracks_url}/b"), OP, None).await.0, StatusCode::NOT_FOUND);
    assert!(state.store.track("b").unwrap().is_some(), "removing from a playlist keeps the track");
    let (_, page) = call(&app, "GET", &format!("/api/v1/playlists/{id}?limit=1"), LIVE, None).await;
    assert_eq!((ids(&page["tracks"]), page["next_cursor"].as_u64()), (vec!["a".to_string()], Some(1)));
    assert_eq!(call(&app, "PATCH", &format!("/api/v1/playlists/{id}"), OP, Some(json!({"max_tracks": 6}))).await.1["error"]["code"], "bad_max_tracks");
    let (_, patched) = call(&app, "PATCH", &format!("/api/v1/playlists/{id}"), OP, Some(json!({"name": "Renamed", "order": "shuffle", "max_tracks": 5}))).await;
    assert_eq!((patched["name"].as_str(), patched["order"].as_str(), patched["max_tracks"].as_u64()), (Some("Renamed"), Some("shuffle"), Some(5)));
}

#[tokio::test]
async fn generative_needs_a_valid_recipe_and_smart_a_valid_rule() {
    let (app, _state, _d) = app().await;
    let post = |b: Value| { let app = app.clone(); async move { call(&app, "POST", "/api/v1/playlists", OP, Some(b)).await } };
    assert_eq!(post(json!({"name": "g", "kind": "generative"})).await.1["error"]["code"], "bad_playlist");
    assert_eq!(post(json!({"name": "g", "kind": "generative", "recipe": {"mood": "angry", "seconds": [20, 30]}})).await.1["error"]["code"], "unknown_mood");
    let (s, g) = post(json!({"name": "g", "kind": "generative", "recipe": {"mood": "calm", "seconds": [20, 30]}})).await;
    assert_eq!((s, g["order"].as_str(), g["recipe"]["library_share"].as_f64()), (StatusCode::CREATED, Some("shuffle"), Some(0.25)));
    assert_eq!(post(json!({"name": "f", "kind": "fixed", "rule": {}})).await.1["error"]["code"], "bad_playlist");
    assert_eq!(post(json!({"name": "s", "kind": "smart", "rule": {"from_playlist": "pl_nope"}})).await.1["error"]["code"], "unknown_playlist");
    assert_eq!(post(json!({"name": "s", "kind": "smart", "rule": {"from_playlist": "kept"}})).await.1["error"]["code"], "unknown_playlist", "from_playlist names a fixed or generative playlist");
    assert_eq!(post(json!({"name": " ", "kind": "fixed"})).await.1["error"]["code"], "bad_name");
    assert_eq!(post(json!({"name": "f", "kind": "fixed", "max_tracks": 0})).await.1["error"]["code"], "bad_max_tracks");
    let (s, v) = post(json!({"name": "v", "kind": "generative", "recipe": {"mood": "calm", "seconds": [20, 30], "vocals": true, "vocal_language": "pt"}})).await;
    assert_eq!((s, v["recipe"]["vocals"].as_bool()), (StatusCode::CREATED, Some(true)));
    let (s, _) = post(json!({"name": "s", "kind": "smart", "rule": {"mood": "calm", "from_playlist": g["id"]}})).await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(call(&app, "POST", &format!("/api/v1/playlists/{}/tracks", g["id"].as_str().unwrap()), OP, Some(json!({"track_ids": []}))).await.0, StatusCode::OK, "generative playlists take tracks by hand");
}

#[tokio::test]
async fn a_smart_playlist_reads_its_matches_newest_first_up_to_its_size() {
    let (app, state, _d) = app().await;
    tracks(&state, &["a", "b", "c", "d"]);
    state.store.insert_track(&sample_track("t", Some("tense"), None, "/x.flac")).unwrap();
    let (_, p) = call(&app, "POST", "/api/v1/playlists", OP, Some(json!({"name": "calm", "kind": "smart", "rule": {"mood": "calm"}}))).await;
    let (_, got) = call(&app, "GET", &format!("/api/v1/playlists/{}", p["id"].as_str().unwrap()), LIVE, None).await;
    assert_eq!((ids(&got["tracks"]), got["count"].as_u64()), (vec!["d".to_string(), "c".to_string(), "b".to_string()], Some(3)));
    assert_eq!(got["seconds"].as_f64(), Some(9.0), "three 3-second songs");
}

#[tokio::test]
async fn a_smart_rule_from_a_deleted_playlist_reads_empty() {
    let (app, state, _d) = app().await;
    tracks(&state, &["a"]);
    let (_, f) = call(&app, "POST", "/api/v1/playlists", OP, Some(json!({"name": "f", "kind": "fixed"}))).await;
    let fid = f["id"].as_str().unwrap().to_string();
    call(&app, "POST", &format!("/api/v1/playlists/{fid}/tracks"), OP, Some(json!({"track_ids": ["a"]}))).await;
    let (_, sp) = call(&app, "POST", "/api/v1/playlists", OP, Some(json!({"name": "s", "kind": "smart", "rule": {"from_playlist": fid}}))).await;
    assert_eq!(call(&app, "DELETE", &format!("/api/v1/playlists/{fid}"), OP, None).await.0, StatusCode::NO_CONTENT);
    let (s, got) = call(&app, "GET", &format!("/api/v1/playlists/{}", sp["id"].as_str().unwrap()), LIVE, None).await;
    assert_eq!((s, got["count"].as_u64()), (StatusCode::OK, Some(0)));
    let (s, renamed) = call(&app, "PATCH", &format!("/api/v1/playlists/{}", sp["id"].as_str().unwrap()), OP, Some(json!({"name": "still here"}))).await;
    assert_eq!((s, renamed["name"].as_str()), (StatusCode::OK, Some("still here")), "a dangling from_playlist does not block other edits");
}

#[tokio::test]
async fn a_playlist_on_air_cannot_be_deleted_and_listeners_change_nothing() {
    let (app, state, _d) = app().await;
    tracks(&state, &["a"]);
    let (_, p) = call(&app, "POST", "/api/v1/playlists", OP, Some(json!({"name": "f", "kind": "fixed"}))).await;
    let id = p["id"].as_str().unwrap().to_string();
    assert_eq!(call(&app, "POST", "/api/v1/playlists", LIVE, Some(json!({"name": "x", "kind": "fixed"}))).await.0, StatusCode::FORBIDDEN);
    assert_eq!(call(&app, "POST", &format!("/api/v1/playlists/{id}/tracks"), LIVE, Some(json!({"track_ids": ["a"]}))).await.0, StatusCode::FORBIDDEN);
    assert_eq!(call(&app, "DELETE", &format!("/api/v1/playlists/{id}"), LIVE, None).await.0, StatusCode::FORBIDDEN);
    state.store.set_on_air("main", Some(&id)).unwrap();
    assert_eq!(call(&app, "DELETE", &format!("/api/v1/playlists/{id}"), OP, None).await.1["error"]["code"], "playlist_on_air");
    state.store.set_on_air("main", None).unwrap();
    assert_eq!(call(&app, "DELETE", &format!("/api/v1/playlists/{id}"), OP, None).await.0, StatusCode::NO_CONTENT);
    assert_eq!(call(&app, "GET", &format!("/api/v1/playlists/{id}"), LIVE, None).await.0, StatusCode::NOT_FOUND);
    assert!(state.store.track("a").unwrap().is_some());
}

#[tokio::test]
async fn a_playlist_over_its_size_can_still_be_reordered_but_not_grown() {
    let (app, state, _d) = app().await;
    tracks(&state, &["a", "b", "c", "d", "e"]);
    let p = common::playlist("pl_big", maio::playlists::Kind::Generative, maio::playlists::Order::InOrder);
    let p = maio::playlists::Playlist { recipe: Some(serde_json::from_value(json!({"mood": "calm", "seconds": [20, 30]})).unwrap()), ..p };
    state.store.insert_playlist(&p).unwrap();
    state.store.add_to_playlist("pl_big", &["a".into(), "b".into(), "c".into(), "d".into()], None).unwrap(); // 4, over the size of 3 (as after a migration)
    assert_eq!(call(&app, "PUT", "/api/v1/playlists/pl_big/tracks", OP, Some(json!({"track_ids": ["d", "c", "b", "a"]}))).await.0, StatusCode::OK);
    assert_eq!(call(&app, "PUT", "/api/v1/playlists/pl_big/tracks", OP, Some(json!({"track_ids": ["d", "c", "b", "a", "e"]}))).await.1["error"]["code"], "playlist_full");
}

#[tokio::test]
async fn editing_the_playlist_on_air_changes_what_plays_next() {
    let (app, state, _d) = app().await;
    tracks(&state, &["a", "b", "c"]);
    let (_, p) = call(&app, "POST", "/api/v1/playlists", OP, Some(json!({"name": "f", "kind": "fixed"}))).await;
    let id = p["id"].as_str().unwrap().to_string();
    call(&app, "PUT", &format!("/api/v1/playlists/{id}/tracks"), OP, Some(json!({"track_ids": ["a", "b", "c"]}))).await;
    call(&app, "PUT", "/api/v1/on-air", OP, Some(json!({"playlist_id": id}))).await;
    let upcoming = |v: &Value| v["upcoming"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect::<Vec<_>>();
    assert_eq!(upcoming(&call(&app, "GET", "/api/v1/on-air", LIVE, None).await.1), ["a", "b", "c"]);
    call(&app, "PUT", &format!("/api/v1/playlists/{id}/tracks"), OP, Some(json!({"track_ids": ["c", "b", "a"]}))).await;
    assert_eq!(upcoming(&call(&app, "GET", "/api/v1/on-air", LIVE, None).await.1), ["c", "b", "a"], "the new order plays now, not after this cycle");
}
