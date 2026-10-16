mod common;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use common::{Outcome, SITE, spawn_fake_engine, test_state};
use maio::api;
use serde_json::{Value, json};
use tower::ServiceExt;

const ME: &str = "sess-root";
const LIVE: &str = "sess-guest";

/// A generative playlist stored straight in the store (the playlists API arrives in Task 3).
fn gen_playlist(state: &maio::state::AppState, id: &str, library_share: f32) -> String {
    let recipe = serde_json::from_value(json!({"mood": "calm", "seconds": [20, 30], "library_share": library_share})).unwrap();
    let p = maio::playlists::Playlist { recipe: Some(recipe), ..common::playlist(id, maio::playlists::Kind::Generative, maio::playlists::Order::Shuffle) };
    state.store.insert_playlist(&p).unwrap();
    id.to_string()
}

async fn call(app: &Router, method: &str, path: &str, token: Option<&str>, body: Option<Value>) -> (StatusCode, Value, axum::http::HeaderMap) {
    let mut req = Request::builder().method(method).uri(path).header("host", "localhost:8080").header("origin", SITE);
    if let Some(t) = token {
        req = req.header("cookie", format!("maio_session={t}"));
    }
    let req = match body {
        Some(b) => req.header("content-type", "application/json").body(Body::from(b.to_string())).unwrap(),
        None => req.body(Body::empty()).unwrap(),
    };
    let res = app.clone().oneshot(req).await.unwrap();
    let (status, headers) = (res.status(), res.headers().clone());
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null), headers)
}

async fn app_with_engine() -> (Router, maio::state::AppState, tempfile::TempDir) {
    let fake = spawn_fake_engine("fake", "yes", Outcome::Hang).await;
    let nc = spawn_fake_engine("nc", "no", Outcome::Hang).await;
    let (state, dir) = test_state(vec![(fake, "fake"), (nc, "nc")], "").await;
    common::person(&state, "root", "admin");
    common::person(&state, "guest", "listener");
    (api::router(state.clone()), state, dir)
}

#[tokio::test]
async fn requests_need_a_signed_in_person_and_bearer_tokens_do_nothing() {
    let (app, _, _d) = app_with_engine().await;
    let (s, body, _) = call(&app, "GET", "/api/v1/status", None, None).await;
    assert_eq!((s, body["error"]["code"].as_str()), (StatusCode::UNAUTHORIZED, Some("unauthorized")));
    assert_eq!(call(&app, "GET", "/api/v1/status", Some("nope"), None).await.0, StatusCode::UNAUTHORIZED);
    let req = Request::get("/api/v1/status").header("authorization", "Bearer tok-T_ME").body(Body::empty()).unwrap();
    assert_eq!(app.clone().oneshot(req).await.unwrap().status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn listeners_cannot_skip() {
    let (app, _, _d) = app_with_engine().await;
    let (s, body, _) = call(&app, "POST", "/api/v1/skip", Some(LIVE), None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "missing_scope");
    assert_eq!(call(&app, "POST", "/api/v1/skip", Some(ME), None).await.0, StatusCode::ACCEPTED);
}

#[tokio::test]
async fn status_shows_moods_genres_and_the_queue() {
    let (app, _, _d) = app_with_engine().await;
    let req = json!({"request": {"mood": "calm", "genre": "lo-fi", "tempo": "slow", "seconds": 180}, "position": "next"});
    assert_eq!(call(&app, "POST", "/api/v1/queue", Some(ME), Some(req)).await.0, StatusCode::ACCEPTED);
    let (s, body, _) = call(&app, "GET", "/api/v1/status", Some(LIVE), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["moods"], json!(["calm", "tense"]));
    assert_eq!(body["genres"], json!(["lo-fi", "synthwave"]));
    assert_eq!(body["playout"]["running"], true);
    assert_eq!(body["queue"][0]["title"], "calm lo-fi, slow");
    assert!(body["queue"][0].get("prompt").is_none() && body["queue"][0].get("mine").is_none());
}

#[tokio::test]
async fn people_are_rate_limited_by_the_limits_section() {
    let fake = spawn_fake_engine("fake", "yes", Outcome::Hang).await;
    let (state, _d) = test_state(vec![(fake, "fake")], "limits = { queue = \"2/10m\" }").await;
    common::person(&state, "root", "admin");
    let app = api::router(state);
    let req = json!({"request": {"mood": "calm"}});
    for _ in 0..2 {
        assert_eq!(call(&app, "POST", "/api/v1/queue", Some(ME), Some(req.clone())).await.0, StatusCode::ACCEPTED);
    }
    let (s, body, headers) = call(&app, "POST", "/api/v1/queue", Some(ME), Some(req)).await;
    assert_eq!((s, body["error"]["code"].as_str()), (StatusCode::TOO_MANY_REQUESTS, Some("rate_limited")));
    assert!(headers.contains_key("retry-after"));
}

#[tokio::test]
async fn there_are_no_token_routes() {
    let (app, _, _d) = app_with_engine().await;
    assert_eq!(call(&app, "GET", "/api/v1/tokens", Some(ME), None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(call(&app, "POST", "/api/v1/tokens", Some(ME), Some(json!({"name": "x1", "scopes": ["read"]}))).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn noncommercial_engine_needs_admin_flag() {
    let (app, _, _d) = app_with_engine().await;
    let body = json!({"request": {"mood": "calm", "engine": "nc"}});
    let (s, err, _) = call(&app, "POST", "/api/v1/queue", Some(ME), Some(body)).await;
    assert_eq!((s, err["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("noncommercial_engine")));
    let ok = json!({"request": {"mood": "calm", "engine": "nc", "allow_noncommercial": true}});
    assert_eq!(call(&app, "POST", "/api/v1/queue", Some(ME), Some(ok)).await.0, StatusCode::ACCEPTED);
}

#[tokio::test]
async fn on_air_switching_rules() {
    let (app, state, _d) = app_with_engine().await;
    let id = gen_playlist(&state, "pl_g", 0.25);
    assert_eq!(call(&app, "PUT", "/api/v1/on-air", Some(LIVE), Some(json!({"playlist_id": id}))).await.0, StatusCode::FORBIDDEN);
    assert_eq!(call(&app, "PUT", "/api/v1/on-air", Some(ME), Some(json!({"playlist_id": "pl_nope"}))).await.0, StatusCode::NOT_FOUND);
    let (s, body, _) = call(&app, "PUT", "/api/v1/on-air", Some(ME), Some(json!({"playlist_id": id}))).await;
    assert_eq!((s, body["playlist"]["id"].as_str()), (StatusCode::OK, Some("pl_g")));
    let (_, got, _) = call(&app, "GET", "/api/v1/on-air", Some(LIVE), None).await;
    assert_eq!((got["playlist"]["kind"].as_str(), got["playlist"]["max_tracks"].as_u64()), (Some("generative"), Some(50)));
    let (_, status, _) = call(&app, "GET", "/api/v1/status", Some(LIVE), None).await;
    assert_eq!(status["on_air"]["id"], "pl_g");
    assert_eq!(call(&app, "PUT", "/api/v1/on-air", Some(ME), Some(json!({"playlist_id": null}))).await.0, StatusCode::OK);
    assert_eq!(call(&app, "GET", "/api/v1/on-air", Some(LIVE), None).await.1, Value::Null);
    assert_eq!(call(&app, "GET", "/api/v1/stations", Some(LIVE), None).await.0, StatusCode::NOT_FOUND, "stations are gone");
}

#[tokio::test]
async fn on_air_shows_the_next_tracks_in_the_order_they_will_play() {
    let (app, state, _d) = app_with_engine().await;
    for id in ["a", "b", "c", "d"] {
        state.store.insert_track(&common::sample_track(id, None, None, "/x.flac")).unwrap();
    }
    let p = common::playlist("pl_s", maio::playlists::Kind::Fixed, maio::playlists::Order::Shuffle);
    state.store.insert_playlist(&p).unwrap();
    state.store.add_to_playlist("pl_s", &["a".into(), "b".into(), "c".into(), "d".into()], None).unwrap();
    call(&app, "PUT", "/api/v1/on-air", Some(ME), Some(json!({"playlist_id": "pl_s"}))).await;
    let (_, got, _) = call(&app, "GET", "/api/v1/on-air", Some(LIVE), None).await;
    let upcoming: Vec<String> = got["upcoming"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    assert_eq!(upcoming.len(), 4);
    maio::playout::fill(&state, &state.main());
    let queued: Vec<String> = state.main().queue.lock().unwrap().upcoming().filter_map(|e| e.track_id.clone()).collect();
    assert_eq!(queued, upcoming[..2], "the drawn order is the one that plays");
}

#[tokio::test]
async fn bad_bodies_get_json_errors() {
    let (app, _, _d) = app_with_engine().await;
    let (s, body, _) = call(&app, "POST", "/api/v1/queue", Some(ME), Some(json!({"request": {"mood": "calm"}, "bogus": 1}))).await;
    assert_eq!((s, body["error"]["code"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some("invalid_body")));
}

#[tokio::test]
async fn events_work_with_the_session_cookie_only() {
    let (app, _, _d) = app_with_engine().await;
    let req = Request::get("/api/v1/events").header("cookie", format!("maio_session={LIVE}")).body(Body::empty()).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["content-type"], "text/event-stream");
    let token = Request::get("/api/v1/events?token=tok-T_LIVE").body(Body::empty()).unwrap();
    assert_eq!(app.clone().oneshot(token).await.unwrap().status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn index_page_and_stream_route() {
    let (app, _, _d) = app_with_engine().await;
    let res = app.clone().oneshot(Request::get("/").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let page = axum::body::to_bytes(res.into_body(), 1 << 22).await.unwrap();
    let page = String::from_utf8(page.to_vec()).unwrap();
    for needle in ["/app.css", "/app.js", "/ui.js", "id=\"signin\"", "data-view=\"now\"", "data-theme-toggle", "Archivo"] {
        assert!(page.contains(needle), "the page has {needle}");
    }
    assert!(!page.contains("Big Shoulders"), "the vinyl-and-tape page is gone");
    let mp3 = app.clone().oneshot(Request::get("/stream.mp3").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(mp3.status(), StatusCode::NOT_FOUND, "mp3 is off unless serve.mp3 is set");
}

#[tokio::test]
async fn library_endpoints() {
    let (app, state, _d) = app_with_engine().await;
    state.store.insert_track(&common::sample_track("trk_a", Some("calm"), None, "/x.flac")).unwrap();
    let (_, list, _) = call(&app, "GET", "/api/v1/library?mood=calm", Some(LIVE), None).await;
    assert_eq!(list["tracks"][0]["id"], "trk_a");
    assert_eq!(call(&app, "PUT", "/api/v1/library/trk_a/favourite", Some(ME), Some(json!({"favourite": true}))).await.0, StatusCode::NO_CONTENT);
    assert_eq!(call(&app, "GET", "/api/v1/library/trk_zzz", Some(ME), None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(call(&app, "POST", "/api/v1/queue", Some(ME), Some(json!({"track_id": "trk_a"}))).await.0, StatusCode::ACCEPTED);
}

#[tokio::test]
async fn switching_the_playlist_on_air_drops_side_b() {
    let (app, state, _d) = app_with_engine().await;
    for id in ["trk_a", "trk_b", "trk_c"] {
        state.store.insert_track(&common::sample_track(id, None, None, "/x.flac")).unwrap();
    }
    let first = gen_playlist(&state, "pl_first", 1.0);
    call(&app, "PUT", "/api/v1/on-air", Some(ME), Some(json!({"playlist_id": first}))).await;
    maio::playout::fill(&state, &state.main());
    assert_eq!(state.main().queue.lock().unwrap().pending_count(), 2);
    let created = gen_playlist(&state, "pl_created", 0.25);
    call(&app, "PUT", "/api/v1/on-air", Some(ME), Some(json!({"playlist_id": created}))).await;
    assert_eq!(state.main().queue.lock().unwrap().pending_count(), 0, "fillers from before the switch are gone");
}

fn add_user(state: &maio::state::AppState, username: &str, role: &str, must_change: bool) -> String {
    state.store.insert_user(&maio::store::User {
        username: username.into(), name: username.into(), role: role.into(),
        hash: maio::users::hash_password("correct horse battery"), must_change,
        created_at: maio::store::now(), last_seen_at: None,
    }).unwrap();
    let id = maio::users::new_secret();
    state.store.create_session(&id, username, i64::MAX / 2, false).unwrap();
    id
}

async fn cookie_call(app: &Router, method: &str, path: &str, session: &str, origin: Option<&str>, body: Option<Value>) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(path).header("host", "localhost:8080")
        .header("cookie", format!("theme=dark; maio_session={session}"));
    if let Some(o) = origin {
        req = req.header("origin", o);
    }
    let req = match body {
        Some(b) => req.header("content-type", "application/json").body(Body::from(b.to_string())).unwrap(),
        None => req.body(Body::empty()).unwrap(),
    };
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

#[tokio::test]
async fn a_session_cookie_acts_with_its_role_and_needs_a_matching_origin_to_change_things() {
    let (app, state, _d) = app_with_engine().await;
    let op = add_user(&state, "ana", "operator", false);
    assert_eq!(cookie_call(&app, "GET", "/api/v1/status", &op, None, None).await.0, StatusCode::OK);
    let (s, body) = cookie_call(&app, "POST", "/api/v1/skip", &op, None, None).await;
    assert_eq!((s, body["error"]["code"].as_str()), (StatusCode::FORBIDDEN, Some("bad_origin")));
    assert_eq!(cookie_call(&app, "POST", "/api/v1/skip", &op, Some("https://evil.example"), None).await.0, StatusCode::FORBIDDEN);
    assert_eq!(cookie_call(&app, "POST", "/api/v1/skip", &op, Some("http://localhost:8080"), None).await.0, StatusCode::ACCEPTED);
    assert_eq!(cookie_call(&app, "POST", "/api/v1/playout/stop", &op, Some("http://localhost:8080"), None).await.0, StatusCode::FORBIDDEN, "operators are not admins");
    let listener = add_user(&state, "lobby", "listener", false);
    assert_eq!(cookie_call(&app, "POST", "/api/v1/skip", &listener, Some("http://localhost:8080"), None).await.0, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn expired_or_removed_sessions_stop_working() {
    let (app, state, _d) = app_with_engine().await;
    let id = add_user(&state, "ana", "operator", false);
    state.store.create_session("stale", "ana", 1, false).unwrap();
    assert_eq!(cookie_call(&app, "GET", "/api/v1/status", "stale", None, None).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(cookie_call(&app, "GET", "/api/v1/status", "made-up", None, None).await.0, StatusCode::UNAUTHORIZED);
    state.store.delete_user("ana").unwrap();
    assert_eq!(cookie_call(&app, "GET", "/api/v1/status", &id, None, None).await.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_temporary_password_blocks_everything_until_changed() {
    let (app, state, _d) = app_with_engine().await;
    let id = add_user(&state, "new", "operator", true);
    let (s, body) = cookie_call(&app, "GET", "/api/v1/status", &id, None, None).await;
    assert_eq!((s, body["error"]["code"].as_str()), (StatusCode::FORBIDDEN, Some("password_change_required")));
}

/// Signs in through the API; returns (status, session id from Set-Cookie, the Set-Cookie header).
async fn sign_in(app: &Router, username: &str, password: &str, remember: bool) -> (StatusCode, Option<String>, String) {
    let req = Request::post("/api/v1/session").header("host", "localhost:8080").header("origin", SITE)
        .header("content-type", "application/json")
        .body(Body::from(json!({"username": username, "password": password, "remember": remember}).to_string())).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let set = res.headers().get("set-cookie").map(|v| v.to_str().unwrap().to_string()).unwrap_or_default();
    let id = set.split(';').next().and_then(|kv| kv.strip_prefix("maio_session=")).filter(|v| !v.is_empty()).map(str::to_string);
    (res.status(), id, set)
}

#[tokio::test]
async fn sign_in_sets_a_strict_cookie_and_sign_out_ends_it() {
    let (app, state, _d) = app_with_engine().await;
    add_user(&state, "ana", "operator", false);
    assert_eq!(sign_in(&app, "ana", "wrong password!!", false).await.0, StatusCode::UNAUTHORIZED);
    let (s, _, _) = sign_in(&app, "nobody", "correct horse battery", false).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "unknown users look the same as wrong passwords");
    let (s, id, set) = sign_in(&app, "ana", "correct horse battery", false).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    for part in ["HttpOnly", "SameSite=Strict", "Path=/", "Max-Age=43200"] {
        assert!(set.contains(part), "{set}");
    }
    assert!(sign_in(&app, "ana", "correct horse battery", true).await.2.contains("Max-Age=2592000"));
    let id = id.unwrap();
    let (s, me) = cookie_call(&app, "GET", "/api/v1/me", &id, None, None).await;
    assert_eq!((s, me["username"].as_str(), me["role"].as_str()), (StatusCode::OK, Some("ana"), Some("operator")));
    assert_eq!(cookie_call(&app, "DELETE", "/api/v1/session", &id, Some(SITE), None).await.0, StatusCode::NO_CONTENT);
    assert_eq!(cookie_call(&app, "GET", "/api/v1/me", &id, None, None).await.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn sign_in_is_rate_limited_per_username() {
    let (app, state, _d) = app_with_engine().await;
    add_user(&state, "ana", "operator", false);
    for _ in 0..5 {
        assert_eq!(sign_in(&app, "ana", "wrong password!!", false).await.0, StatusCode::UNAUTHORIZED);
    }
    assert_eq!(sign_in(&app, "ana", "correct horse battery", false).await.0, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn changing_a_password_clears_the_flag_and_ends_other_sessions() {
    let (app, state, _d) = app_with_engine().await;
    let id = add_user(&state, "new", "operator", true);
    state.store.create_session("other", "new", i64::MAX / 2, false).unwrap();
    let wrong = json!({"current": "not my password", "new": "a brand new password"});
    assert_eq!(cookie_call(&app, "POST", "/api/v1/me/password", &id, Some(SITE), Some(wrong)).await.0, StatusCode::FORBIDDEN);
    let weak = json!({"current": "correct horse battery", "new": "short"});
    assert_eq!(cookie_call(&app, "POST", "/api/v1/me/password", &id, Some(SITE), Some(weak)).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    let ok = json!({"current": "correct horse battery", "new": "a brand new password"});
    assert_eq!(cookie_call(&app, "POST", "/api/v1/me/password", &id, Some(SITE), Some(ok)).await.0, StatusCode::NO_CONTENT);
    assert_eq!(cookie_call(&app, "GET", "/api/v1/status", &id, None, None).await.0, StatusCode::OK);
    assert_eq!(cookie_call(&app, "GET", "/api/v1/status", "other", None, None).await.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admins_manage_users_but_never_lose_the_last_admin() {
    let (app, state, _d) = app_with_engine().await;
    let op = add_user(&state, "ana", "operator", false);
    let new = json!({"name": "Bo", "username": "bo", "password": "temporary pass 1", "role": "listener"});
    assert_eq!(cookie_call(&app, "POST", "/api/v1/users", &op, Some(SITE), Some(new.clone())).await.0, StatusCode::FORBIDDEN);
    let (s, body) = cookie_call(&app, "POST", "/api/v1/users", ME, Some(SITE), Some(new.clone())).await;
    assert_eq!((s, body["must_change"].as_bool()), (StatusCode::CREATED, Some(true)));
    assert!(body.get("hash").is_none());
    assert_eq!(cookie_call(&app, "POST", "/api/v1/users", ME, Some(SITE), Some(new)).await.0, StatusCode::CONFLICT);
    let bad = json!({"name": "X", "username": "Bad Name", "password": "temporary pass 1", "role": "listener"});
    assert_eq!(cookie_call(&app, "POST", "/api/v1/users", ME, Some(SITE), Some(bad)).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    let (_, list) = cookie_call(&app, "GET", "/api/v1/users", ME, None, None).await;
    assert_eq!(list.as_array().unwrap().len(), 4);

    let (s, body) = cookie_call(&app, "PATCH", "/api/v1/users/root", ME, Some(SITE), Some(json!({"role": "operator"}))).await;
    assert_eq!((s, body["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("last_admin")));
    let (s, body) = cookie_call(&app, "DELETE", "/api/v1/users/root", ME, Some(SITE), None).await;
    assert_eq!((s, body["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("cannot_remove_self")));

    assert_eq!(cookie_call(&app, "POST", "/api/v1/users/ana/password", ME, Some(SITE), Some(json!({"password": "reset password 1"}))).await.0, StatusCode::NO_CONTENT);
    assert_eq!(cookie_call(&app, "GET", "/api/v1/status", &op, None, None).await.0, StatusCode::UNAUTHORIZED, "a reset ends the user's sessions");
    assert!(state.store.user("ana").unwrap().unwrap().must_change);

    assert_eq!(cookie_call(&app, "PATCH", "/api/v1/users/ana", ME, Some(SITE), Some(json!({"role": "admin"}))).await.0, StatusCode::OK);
    assert_eq!(cookie_call(&app, "PATCH", "/api/v1/users/root", ME, Some(SITE), Some(json!({"role": "operator"}))).await.0, StatusCode::OK, "another admin exists now");
    assert_eq!(cookie_call(&app, "DELETE", "/api/v1/users/bo", ME, Some(SITE), None).await.0, StatusCode::FORBIDDEN, "root is no longer an admin");
}

#[tokio::test]
async fn clearing_side_a_keeps_side_b() {
    let (app, state, _d) = app_with_engine().await;
    for id in ["trk_a", "trk_b", "trk_c"] {
        state.store.insert_track(&common::sample_track(id, None, None, "/x.flac")).unwrap();
    }
    let st = gen_playlist(&state, "pl_st", 1.0);
    call(&app, "PUT", "/api/v1/on-air", Some(ME), Some(json!({"playlist_id": st}))).await;
    maio::playout::fill(&state, &state.main());
    assert_eq!(call(&app, "POST", "/api/v1/queue", Some(ME), Some(json!({"track_id": "trk_c"}))).await.0, StatusCode::ACCEPTED);
    let (_, before, _) = call(&app, "GET", "/api/v1/queue", Some(ME), None).await;
    let sides: Vec<_> = before.as_array().unwrap().iter().map(|e| e["side"].as_str().unwrap().to_string()).collect();
    assert_eq!(sides, ["a", "b", "b"]);
    assert!(before[0]["seed"].is_i64());
    assert_eq!(call(&app, "DELETE", "/api/v1/queue", Some(LIVE), None).await.0, StatusCode::FORBIDDEN);
    assert_eq!(call(&app, "DELETE", "/api/v1/queue", Some(ME), None).await.0, StatusCode::NO_CONTENT);
    let (_, after, _) = call(&app, "GET", "/api/v1/queue", Some(ME), None).await;
    assert_eq!(after.as_array().unwrap().len(), 2);
    assert!(after.as_array().unwrap().iter().all(|e| e["side"] == "b"));
}

#[tokio::test]
async fn sign_in_with_an_invalid_username_is_refused_without_state() {
    let (app, state, _d) = app_with_engine().await;
    let long = "x".repeat(5000);
    let (s, _, _) = sign_in(&app, &long, "correct horse battery", false).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert_eq!(state.signin.keys(), 0, "invalid usernames never enter the limiter");
}

#[tokio::test]
async fn the_temporary_password_gate_matches_exact_paths() {
    let (app, state, _d) = app_with_engine().await;
    add_user(&state, "me", "listener", false);
    let admin = add_user(&state, "boss", "admin", true);
    let (s, body) = cookie_call(&app, "POST", "/api/v1/users/me/password", &admin, Some(SITE), Some(json!({"password": "taken over pass"}))).await;
    assert_eq!((s, body["error"]["code"].as_str()), (StatusCode::FORBIDDEN, Some("password_change_required")));
    assert_eq!(cookie_call(&app, "GET", "/api/v1/me", &admin, None, None).await.0, StatusCode::OK);
}

#[tokio::test]
async fn aac_stream_route_serves_audio_aac_when_enabled() {
    let (state, _d) = test_state(vec![], "serve = { aac = true }").await;
    let app = api::router(state);
    let res = app.clone().oneshot(Request::get("/stream.aac").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()["content-type"], "audio/aac");
    let mp3 = app.oneshot(Request::get("/stream.mp3").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(mp3.status(), StatusCode::NOT_FOUND, "only the enabled formats are served");
}


#[tokio::test]
async fn clearing_side_b_empties_it_and_takes_the_playlist_off_air() {
    let (app, state, _d) = app_with_engine().await;
    for id in ["trk_a", "trk_b", "trk_c"] {
        state.store.insert_track(&common::sample_track(id, None, None, "/x.flac")).unwrap();
    }
    let st = gen_playlist(&state, "pl_st", 1.0);
    call(&app, "PUT", "/api/v1/on-air", Some(ME), Some(json!({"playlist_id": st}))).await;
    maio::playout::fill(&state, &state.main());
    call(&app, "POST", "/api/v1/queue", Some(ME), Some(json!({"track_id": "trk_c"}))).await;
    assert_eq!(call(&app, "DELETE", "/api/v1/queue?side=b", Some(LIVE), None).await.0, StatusCode::FORBIDDEN);
    assert_eq!(call(&app, "DELETE", "/api/v1/queue?side=b", Some(ME), None).await.0, StatusCode::NO_CONTENT);
    let (_, q, _) = call(&app, "GET", "/api/v1/queue", Some(ME), None).await;
    assert!(q.as_array().unwrap().iter().all(|e| e["side"] == "a"), "only side A is left");
    assert_eq!(q.as_array().unwrap().len(), 1);
    assert!(state.store.on_air("main").unwrap().is_none(), "nothing on air, so side B stays empty");
    maio::playout::fill(&state, &state.main());
    assert_eq!(state.main().queue.lock().unwrap().pending_count(), 1);
}

#[tokio::test]
async fn deleting_one_track_removes_its_file_but_never_a_queued_one() {
    let (app, state, dir) = app_with_engine().await;
    let file = dir.path().join("one.flac");
    std::fs::write(&file, b"x").unwrap();
    state.store.insert_track(&common::sample_track("trk_one", None, None, file.to_str().unwrap())).unwrap();
    state.store.insert_track(&common::sample_track("trk_q", None, None, "/x.flac")).unwrap();
    call(&app, "POST", "/api/v1/queue", Some(ME), Some(json!({"track_id": "trk_q"}))).await;
    assert_eq!(call(&app, "DELETE", "/api/v1/library/trk_one", Some(LIVE), None).await.0, StatusCode::FORBIDDEN);
    assert_eq!(call(&app, "DELETE", "/api/v1/library/trk_one", Some(ME), None).await.0, StatusCode::NO_CONTENT);
    assert!(state.store.track("trk_one").unwrap().is_none());
    assert!(!file.exists(), "the file is gone too");
    let (s, body, _) = call(&app, "DELETE", "/api/v1/library/trk_q", Some(ME), None).await;
    assert_eq!((s, body["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("track_in_use")));
    assert_eq!(call(&app, "DELETE", "/api/v1/library/trk_zzz", Some(ME), None).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn emptying_the_crate_keeps_kept_and_queued_tracks_and_is_admin_only() {
    let (app, state, _d) = app_with_engine().await;
    for id in ["trk_kept", "trk_gone", "trk_queued"] {
        state.store.insert_track(&common::sample_track(id, None, None, "/nonexistent/x.flac")).unwrap();
    }
    state.store.set_favourite("trk_kept", true).unwrap();
    call(&app, "POST", "/api/v1/queue", Some(ME), Some(json!({"track_id": "trk_queued"}))).await;
    let op = add_user(&state, "ana", "operator", false);
    assert_eq!(cookie_call(&app, "DELETE", "/api/v1/library", &op, Some(SITE), None).await.0, StatusCode::FORBIDDEN);
    let (s, body, _) = call(&app, "DELETE", "/api/v1/library", Some(ME), None).await;
    assert_eq!((s, body["deleted"].as_u64()), (StatusCode::OK, Some(1)));
    assert!(state.store.track("trk_gone").unwrap().is_none());
    assert!(state.store.track("trk_kept").unwrap().is_some());
    assert!(state.store.track("trk_queued").unwrap().is_some());
}

#[tokio::test]
async fn the_crate_filters_by_side_and_playlist() {
    let (app, state, _d) = app_with_engine().await;
    state.store.insert_track(&common::sample_track("trk_asked", None, None, "/x.flac")).unwrap();
    state.store.insert_track(&common::sample_track("trk_st", None, Some("st_1"), "/x.flac")).unwrap();
    let ids = |v: &Value| v["tracks"].as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    assert_eq!(ids(&call(&app, "GET", "/api/v1/library?side=a", Some(LIVE), None).await.1), ["trk_asked"]);
    assert_eq!(ids(&call(&app, "GET", "/api/v1/library?side=b", Some(LIVE), None).await.1), ["trk_st"]);
    state.store.insert_playlist(&common::playlist("pl_f", maio::playlists::Kind::Fixed, maio::playlists::Order::InOrder)).unwrap();
    state.store.add_to_playlist("pl_f", &["trk_asked".into()], None).unwrap();
    assert_eq!(ids(&call(&app, "GET", "/api/v1/library?playlist=pl_f", Some(LIVE), None).await.1), ["trk_asked"]);
    assert_eq!(ids(&call(&app, "GET", "/api/v1/library?playlist=requests", Some(LIVE), None).await.1), ["trk_asked"]);
    assert_eq!(call(&app, "GET", "/api/v1/library?playlist=pl_nope", Some(LIVE), None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(call(&app, "GET", "/api/v1/library?side=c", Some(LIVE), None).await.0, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn renaming_a_track_sets_a_persons_title() {
    let (app, state, _d) = app_with_engine().await;
    state.store.insert_track(&common::sample_track("trk_a", None, None, "/nonexistent/a.opus")).unwrap();
    let body = |t: &str| Some(json!({"title": t}));
    assert_eq!(call(&app, "PATCH", "/api/v1/library/trk_a", Some(LIVE), body("x")).await.0, StatusCode::FORBIDDEN);
    assert_eq!(call(&app, "PATCH", "/api/v1/library/trk_a", Some(ME), body("  ")).await.1["error"]["code"], "bad_title");
    assert_eq!(call(&app, "PATCH", "/api/v1/library/trk_a", Some(ME), body("two\nlines")).await.1["error"]["code"], "bad_title");
    assert_eq!(call(&app, "PATCH", "/api/v1/library/trk_a", Some(ME), body(&"x".repeat(81))).await.1["error"]["code"], "bad_title");
    assert_eq!(call(&app, "PATCH", "/api/v1/library/trk_zzz", Some(ME), body("x")).await.0, StatusCode::NOT_FOUND);
    let (s, t, _) = call(&app, "PATCH", "/api/v1/library/trk_a", Some(ME), body(" Night Bus ")).await;
    assert_eq!((s, t["title"].as_str(), t["title_source"].as_str()), (StatusCode::OK, Some("Night Bus"), Some("person")));
    let (_, got, _) = call(&app, "GET", "/api/v1/library/trk_a", Some(LIVE), None).await;
    assert_eq!((got["artist"].as_str(), got["album"].as_str()), (Some("Maio"), Some("Requests")));
}

#[tokio::test]
async fn renaming_a_track_rewrites_its_tags_and_survives_a_missing_file() {
    let (app, state, dir) = app_with_engine().await;
    let wav = dir.path().join("a.wav");
    maio::media::test_tone(&wav, 1).await.unwrap();
    let file = dir.path().join("a.opus");
    maio::media::import(&wav, &file, &Default::default()).await.unwrap();
    state.store.insert_track(&common::sample_track("trk_a", None, None, file.to_str().unwrap())).unwrap();
    state.store.insert_track(&common::sample_track("trk_gone", None, None, "/nonexistent/gone.opus")).unwrap();
    assert_eq!(call(&app, "PATCH", "/api/v1/library/trk_a", Some(ME), Some(json!({"title": "Night Bus"}))).await.0, StatusCode::OK);
    let tags = maio::media::read_tags(&file).await.unwrap();
    assert_eq!(tags.get("title").map(String::as_str), Some("Night Bus"));
    let (s, t, _) = call(&app, "PATCH", "/api/v1/library/trk_gone", Some(ME), Some(json!({"title": "Still renamed"}))).await;
    assert_eq!((s, t["title"].as_str()), (StatusCode::OK, Some("Still renamed")), "a missing file does not block a rename");
}

#[tokio::test]
async fn mp3_listeners_that_ask_get_in_stream_titles() {
    let (state, _d) = test_state(vec![], "serve = { mp3 = true, aac = true }").await;
    let app = api::router(state);
    let ask = Request::get("/stream.mp3").header("icy-metadata", "1").body(Body::empty()).unwrap();
    let res = app.clone().oneshot(ask).await.unwrap();
    assert_eq!((res.status(), res.headers().get("icy-metaint").map(|v| v.to_str().unwrap().to_string())), (StatusCode::OK, Some("16000".to_string())));
    let aac = Request::get("/radios/main/stream.aac").header("Icy-MetaData", "1").body(Body::empty()).unwrap();
    assert!(app.clone().oneshot(aac).await.unwrap().headers().contains_key("icy-metaint"));
    let plain = app.oneshot(Request::get("/stream.mp3").body(Body::empty()).unwrap()).await.unwrap();
    assert!(!plain.headers().contains_key("icy-metaint"), "players that do not ask get the plain stream");
}

#[tokio::test]
async fn engines_lists_text_engines_next_to_music_engines() {
    let fake = common::spawn_fake_text(common::FakeReply::Text("OK")).await;
    let (state, _d) = test_state(vec![], &common::text_engine_toml("local", "openai", &fake.url)).await;
    common::person(&state, "guest", "listener");
    let app = api::router(state);
    let (s, list, _) = call(&app, "GET", "/api/v1/engines", Some(LIVE), None).await;
    assert_eq!(s, StatusCode::OK);
    let local = list.as_array().unwrap().iter().find(|e| e["id"] == "local").unwrap().clone();
    assert_eq!((local["type"].as_str(), local["kind"].as_str(), local["health"].as_str()), (Some("text"), Some("openai"), Some("unknown")));
}

#[tokio::test]
async fn the_crate_sorts_pages_and_says_when_each_song_last_played() {
    let (app, state, _d) = app_with_engine().await;
    for id in ["trk_a", "trk_b", "trk_c"] {
        state.store.insert_track(&common::sample_track(id, None, None, "/x.flac")).unwrap();
    }
    state.store.play_started("trk_a", "A", "main").unwrap();
    let (_, page, _) = call(&app, "GET", "/api/v1/library?sort=played&limit=2", Some(LIVE), None).await;
    assert_eq!((page["tracks"][0]["id"].as_str(), page["next_cursor"].as_u64()), (Some("trk_a"), Some(2)));
    assert!(page["tracks"][0]["last_played_at"].is_string() && page["tracks"][1]["last_played_at"].is_null());
    let (_, rest, _) = call(&app, "GET", "/api/v1/library?sort=played&limit=2&cursor=2", Some(LIVE), None).await;
    assert_eq!((rest["tracks"].as_array().unwrap().len(), rest["next_cursor"].clone()), (1, Value::Null));
    assert_eq!(call(&app, "GET", "/api/v1/library?sort=loudest", Some(LIVE), None).await.1["error"]["code"], "bad_sort");
}

#[tokio::test]
async fn the_page_its_styles_and_scripts_are_served() {
    let (app, _, _d) = app_with_engine().await;
    for (path, kind) in [("/app.css", "text/css"), ("/app.js", "text/javascript"), ("/ui.js", "text/javascript")] {
        let res = app.clone().oneshot(Request::get(path).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK, "{path}");
        assert!(res.headers()["content-type"].to_str().unwrap().starts_with(kind), "{path}");
    }
}

#[tokio::test]
async fn queued_songs_say_who_asked() {
    let (app, state, _d) = app_with_engine().await;
    state.store.insert_track(&common::sample_track("trk_a", None, None, "/x.flac")).unwrap();
    call(&app, "POST", "/api/v1/queue", Some(ME), Some(json!({"track_id": "trk_a"}))).await;
    assert_eq!(call(&app, "GET", "/api/v1/queue", Some(LIVE), None).await.1[0]["by"], "user:root");
}

#[test]
fn the_page_never_builds_html_from_data() {
    let js = include_str!("../web/app.js");
    for (i, line) in js.lines().enumerate() {
        let l = line.trim();
        if l.contains(".innerHTML") || l.contains("insertAdjacentHTML") || l.contains("outerHTML") {
            assert!(l.contains("t.innerHTML = markup"), "line {}: only icon() may set HTML, from fixed markup: {l}", i + 1);
        }
    }
    assert!(js.contains("function h("), "DOM is built with h()");
}

#[test]
fn listener_controls_are_marked_with_their_scope() {
    let html = include_str!("../web/index.html");
    for id in ["skip-btn", "keep-btn", "ask-form", "clear-a-btn", "clear-b-btn"] {
        let at = html.find(&format!("id=\"{id}\"")).unwrap_or_else(|| panic!("{id} is on the page"));
        let tag_start = html[..at].rfind('<').unwrap();
        let tag = &html[tag_start..at + html[at..].find('>').unwrap()];
        assert!(tag.contains("data-need="), "{id} says which scope it needs: {tag}");
    }
}
