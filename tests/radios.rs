mod common;

use std::time::Duration;

use axum::{Router, body::Body, http::{Request, StatusCode}, response::IntoResponse};
use common::{SITE, person, sample_track, test_state};
use maio::api;
use serde_json::{Value, json};
use tower::ServiceExt;

const ADMIN: &str = "sess-root";
const OP: &str = "sess-ana";

async fn call(app: &Router, method: &str, path: &str, session: &str, body: Option<Value>) -> (StatusCode, Value) {
    let req = Request::builder().method(method).uri(path).header("host", "localhost:8080").header("origin", SITE)
        .header("cookie", format!("maio_session={session}"));
    let req = match body {
        Some(b) => req.header("content-type", "application/json").body(Body::from(b.to_string())).unwrap(),
        None => req.body(Body::empty()).unwrap(),
    };
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn app(extra: &str) -> (Router, maio::state::AppState, tempfile::TempDir) {
    let (state, dir) = test_state(vec![], extra).await;
    person(&state, "root", "admin");
    person(&state, "ana", "operator");
    (api::router(state.clone()), state, dir)
}

#[tokio::test]
async fn admins_create_list_rename_and_delete_radios_within_the_limit() {
    let (app, _state, _d) = app("max_radios = 2").await;
    let (s, list) = call(&app, "GET", "/api/v1/radios", OP, None).await;
    assert_eq!((s, list["radios"][0]["id"].as_str()), (StatusCode::OK, Some("main")));
    let lobby = json!({"id": "lobby", "name": "Lobby"});
    assert_eq!(call(&app, "POST", "/api/v1/radios", OP, Some(lobby.clone())).await.0, StatusCode::FORBIDDEN);
    let (s, body) = call(&app, "POST", "/api/v1/radios", ADMIN, Some(json!({"id": "Bad Id", "name": "x"}))).await;
    assert_eq!((s, body["error"]["code"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some("bad_radio_id")));
    let (s, body) = call(&app, "POST", "/api/v1/radios", ADMIN, Some(lobby.clone())).await;
    assert_eq!((s, body["id"].as_str(), body["visibility"].as_str()), (StatusCode::CREATED, Some("lobby"), Some("public")));
    let (s, body) = call(&app, "POST", "/api/v1/radios", ADMIN, Some(lobby)).await;
    assert_eq!((s, body["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("radio_exists")));
    let (s, body) = call(&app, "POST", "/api/v1/radios", ADMIN, Some(json!({"id": "night", "name": "Night"}))).await;
    assert_eq!((s, body["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("radio_limit")));
    let (s, body) = call(&app, "PATCH", "/api/v1/radios/lobby", ADMIN, Some(json!({"name": "Front desk"}))).await;
    assert_eq!((s, body["name"].as_str()), (StatusCode::OK, Some("Front desk")));
    let (s, body) = call(&app, "DELETE", "/api/v1/radios/main", ADMIN, None).await;
    assert_eq!((s, body["error"]["code"].as_str()), (StatusCode::CONFLICT, Some("cannot_remove_main")));
    assert_eq!(call(&app, "DELETE", "/api/v1/radios/lobby", ADMIN, None).await.0, StatusCode::NO_CONTENT);
    assert_eq!(call(&app, "GET", "/api/v1/radios/lobby", OP, None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(call(&app, "GET", "/api/v1/radios/lobby/status", OP, None).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn each_radio_has_its_own_queue_on_air_and_history_and_old_paths_mean_main() {
    let (app, state, _d) = app("").await;
    state.store.insert_track(&sample_track("trk_a", None, None, "/x.flac")).unwrap();
    call(&app, "POST", "/api/v1/radios", ADMIN, Some(json!({"id": "lobby", "name": "Lobby"}))).await;
    assert_eq!(call(&app, "POST", "/api/v1/radios/lobby/queue", OP, Some(json!({"track_id": "trk_a"}))).await.0, StatusCode::ACCEPTED);
    assert_eq!(call(&app, "GET", "/api/v1/radios/lobby/queue", OP, None).await.1.as_array().unwrap().len(), 1);
    assert!(call(&app, "GET", "/api/v1/queue", OP, None).await.1.as_array().unwrap().is_empty(), "main's queue is separate");
    assert!(call(&app, "GET", "/api/v1/radios/main/queue", OP, None).await.1.as_array().unwrap().is_empty());
    let p = common::playlist("pl_g", maio::playlists::Kind::Fixed, maio::playlists::Order::InOrder);
    state.store.insert_playlist(&p).unwrap();
    assert_eq!(call(&app, "PUT", "/api/v1/radios/lobby/on-air", OP, Some(json!({"playlist_id": "pl_g"}))).await.0, StatusCode::OK);
    assert_eq!(call(&app, "GET", "/api/v1/radios/lobby/on-air", OP, None).await.1["playlist"]["id"], "pl_g");
    assert_eq!(call(&app, "GET", "/api/v1/on-air", OP, None).await.1, Value::Null, "main has nothing on air");
    let (_, status) = call(&app, "GET", "/api/v1/radios/lobby/status", OP, None).await;
    assert_eq!((status["radio"]["id"].as_str(), status["on_air"]["id"].as_str()), (Some("lobby"), Some("pl_g")));
    state.store.play_started("trk_a", "A", "lobby").unwrap();
    assert_eq!(call(&app, "GET", "/api/v1/radios/lobby/history", OP, None).await.1["plays"].as_array().unwrap().len(), 1);
    assert!(call(&app, "GET", "/api/v1/history", OP, None).await.1["plays"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn deleting_a_radio_ends_its_streams_and_drops_its_queue() {
    let (app, state, _d) = app("serve = { mp3 = true }").await;
    state.store.insert_track(&sample_track("trk_a", None, None, "/x.flac")).unwrap();
    call(&app, "POST", "/api/v1/radios", ADMIN, Some(json!({"id": "lobby", "name": "Lobby"}))).await;
    call(&app, "POST", "/api/v1/radios/lobby/queue", OP, Some(json!({"track_id": "trk_a"}))).await;
    let res = app.clone().oneshot(Request::get("/radios/lobby/stream.mp3").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let lobby = state.radio("lobby").unwrap();
    assert_eq!(call(&app, "DELETE", "/api/v1/radios/lobby", ADMIN, None).await.0, StatusCode::NO_CONTENT);
    let drained = tokio::time::timeout(Duration::from_secs(3), axum::body::to_bytes(res.into_body(), usize::MAX)).await;
    assert!(drained.is_ok(), "the listener's stream ends");
    assert!(lobby.is_closed());
    assert!(state.busy_track_ids().is_empty(), "its queue no longer protects tracks");
}

#[tokio::test]
async fn events_belong_to_their_radio() {
    let (_app, state, _d) = app("").await;
    let mut rx = state.events.subscribe();
    state.emit(Some("lobby"), "queue", json!([]));
    state.emit(None, "radios", json!([]));
    let a = rx.recv().await.unwrap();
    let b = rx.recv().await.unwrap();
    assert_eq!((a.radio.as_deref(), b.radio.as_deref()), (Some("lobby"), None));
}

async fn get(app: &Router, path: &str) -> StatusCode {
    app.clone().oneshot(Request::get(path).body(Body::empty()).unwrap()).await.unwrap().status()
}

#[tokio::test]
async fn private_radios_listen_only_through_their_secret_link() {
    let (app, _state, _d) = app("serve = { mp3 = true, aac = true }").await;
    let (_, main) = call(&app, "GET", "/api/v1/radios/main", OP, None).await;
    assert_eq!(main["listen"]["mp3"], "/radios/main/stream.mp3");
    assert_eq!(get(&app, "/stream.mp3").await, StatusCode::OK);
    let (s, main) = call(&app, "PATCH", "/api/v1/radios/main", ADMIN, Some(json!({"visibility": "private"}))).await;
    assert_eq!(s, StatusCode::OK);
    let link = main["listen"]["mp3"].as_str().unwrap().to_string();
    assert!(link.starts_with("/listen/") && link.ends_with("/stream.mp3") && link.len() == "/listen/".len() + 32 + "/stream.mp3".len(), "{link}");
    assert_eq!(get(&app, "/stream.mp3").await, StatusCode::NOT_FOUND);
    assert_eq!(get(&app, "/radios/main/stream.mp3").await, StatusCode::NOT_FOUND);
    assert_eq!(get(&app, &link).await, StatusCode::OK);
    let (_, fresh) = call(&app, "POST", "/api/v1/radios/main/listen-key", ADMIN, None).await;
    assert_ne!(fresh["listen"]["mp3"], json!(link));
    assert_eq!(get(&app, &link).await, StatusCode::NOT_FOUND, "the old link stops working");
    assert_eq!(get(&app, fresh["listen"]["mp3"].as_str().unwrap()).await, StatusCode::OK);
    assert_eq!(call(&app, "POST", "/api/v1/radios/main/listen-key", OP, None).await.0, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn a_new_listen_key_ends_open_streams() {
    let (app, state, _d) = app("serve = { mp3 = true }").await;
    call(&app, "PATCH", "/api/v1/radios/main", ADMIN, Some(json!({"visibility": "private"}))).await;
    let link = call(&app, "GET", "/api/v1/radios/main", OP, None).await.1["listen"]["mp3"].as_str().unwrap().to_string();
    let res = app.clone().oneshot(Request::get(&link).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(state.main().listeners.load(std::sync::atomic::Ordering::Relaxed), 1);
    call(&app, "POST", "/api/v1/radios/main/listen-key", ADMIN, None).await;
    let drained = tokio::time::timeout(Duration::from_secs(3), axum::body::to_bytes(res.into_body(), usize::MAX)).await;
    assert!(drained.is_ok(), "the old listener is dropped");
    assert_eq!(state.main().listeners.load(std::sync::atomic::Ordering::Relaxed), 0);
}

#[tokio::test]
async fn hls_files_never_leave_the_radio_folder() {
    let (app, state, _d) = app("serve = { hls = true }").await;
    std::fs::create_dir_all(state.hls_dir("main")).unwrap();
    std::fs::write(state.hls_dir("main").join("index.m3u8"), "#EXTM3U").unwrap();
    std::fs::write(state.hls_dir("main").join(".hidden"), "x").unwrap();
    assert_eq!(get(&app, "/radios/main/hls/index.m3u8").await, StatusCode::OK);
    assert_eq!(get(&app, "/hls/index.m3u8").await, StatusCode::OK);
    for bad in ["/radios/main/hls/..%2F..%2Fmaio.db", "/radios/main/hls/.hidden", "/radios/main/hls/%2E%2E", "/radios/nope/hls/index.m3u8"] {
        assert_eq!(get(&app, bad).await, StatusCode::NOT_FOUND, "{bad}");
    }
}

#[tokio::test]
async fn formats_not_served_have_no_route() {
    let (app, _state, _d) = app("serve = { mp3 = true }").await;
    assert_eq!(get(&app, "/radios/main/stream.aac").await, StatusCode::NOT_FOUND);
    let (_, main) = call(&app, "GET", "/api/v1/radios/main", OP, None).await;
    assert!(main["listen"].get("aac").is_none());
}

#[tokio::test]
async fn the_relay_lists_rtsp_srt_and_rtmp_addresses_on_the_radio_path() {
    let (app, _state, _d) = app("serve = { mp3 = true }\nrelay = { enabled = true, host = \"radio.lan\" }").await;
    let (_, main) = call(&app, "GET", "/api/v1/radios/main", OP, None).await;
    assert_eq!(main["listen"]["rtsp"], "rtsp://radio.lan:8554/radios/main");
    assert_eq!(main["listen"]["srt"], "srt://radio.lan:8890?streamid=read:radios/main");
    assert_eq!(main["listen"]["rtmp"], "rtmp://radio.lan:1935/radios/main");
    let (_, main) = call(&app, "PATCH", "/api/v1/radios/main", ADMIN, Some(json!({"visibility": "private"}))).await;
    let key = main["listen"]["mp3"].as_str().unwrap().split('/').nth(2).unwrap().to_string();
    assert_eq!(main["listen"]["rtsp"], json!(format!("rtsp://radio.lan:8554/listen/{key}")));
}

#[tokio::test]
async fn without_the_relay_there_are_only_http_addresses() {
    let (app, _state, _d) = app("serve = { mp3 = true }").await;
    let (_, main) = call(&app, "GET", "/api/v1/radios/main", OP, None).await;
    assert!(main["listen"].get("rtsp").is_none() && main["listen"].get("srt").is_none());
}

async fn relay_auth(app: &Router, body: Value) -> StatusCode {
    let req = Request::post("/relay/auth").header("content-type", "application/json").body(Body::from(body.to_string())).unwrap();
    app.clone().oneshot(req).await.unwrap().status()
}

#[tokio::test]
async fn mediamtx_asks_maio_who_may_publish_and_read() {
    let (app, state, _d) = app("relay = { enabled = true }").await;
    let secret = state.relay_secret.clone();
    let ok = StatusCode::OK;
    let no = StatusCode::UNAUTHORIZED;
    assert_eq!(relay_auth(&app, json!({"user": "maio", "password": secret, "action": "publish", "path": "radios/main"})).await, ok);
    assert_eq!(relay_auth(&app, json!({"user": "maio", "password": secret, "action": "api", "path": ""})).await, ok);
    assert_eq!(relay_auth(&app, json!({"user": "", "password": "", "action": "publish", "path": "radios/main"})).await, no);
    assert_eq!(relay_auth(&app, json!({"user": "maio", "password": "guess", "action": "publish", "path": "radios/main"})).await, no);
    assert_eq!(relay_auth(&app, json!({"user": "", "password": "", "action": "read", "path": "radios/main"})).await, ok);
    assert_eq!(relay_auth(&app, json!({"user": "", "password": "", "action": "read", "path": "radios/nope"})).await, no);
    call(&app, "PATCH", "/api/v1/radios/main", ADMIN, Some(json!({"visibility": "private"}))).await;
    let key = state.main().row.lock().unwrap().listen_key.clone();
    assert_eq!(relay_auth(&app, json!({"user": "", "password": "", "action": "read", "path": "radios/main"})).await, no, "a private radio is not readable by name");
    assert_eq!(relay_auth(&app, json!({"user": "", "password": "", "action": "read", "path": format!("listen/{key}")})).await, ok);
    assert_eq!(relay_auth(&app, json!({"user": "", "password": "", "action": "playback", "path": format!("listen/{key}")})).await, no);
}

/// A fake MediaMTX control API: one reader of each kind on radios/main; records kicks.
async fn fake_mediamtx() -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
    use axum::{extract::Path, routing::{get, post}};
    let kicks = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let path = json!({"name": "radios/main", "readers": [{"type": "rtspSession", "id": "s1"}, {"type": "srtConn", "id": "c1"}, {"type": "rtmpConn", "id": "r1"}]});
    let (list, one) = (path.clone(), path);
    let k = kicks.clone();
    let app = Router::new()
        .route("/v3/paths/list", get(move || { let v = json!({"items": [list.clone()]}); async move { axum::Json(v) } }))
        .route("/v3/paths/get/{*name}", get(move |Path(name): Path<String>| {
            let v = one.clone();
            async move { if name == "radios/main" { axum::Json(v).into_response() } else { StatusCode::NOT_FOUND.into_response() } }
        }))
        .route("/v3/{kind}/{what}/kick/{id}", post(move |Path((kind, _what, id)): Path<(String, String, String)>| {
            k.lock().unwrap().push(format!("{kind}:{id}"));
            async { StatusCode::OK }
        }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, kicks)
}

#[tokio::test]
async fn relay_readers_count_as_listeners_and_a_new_key_kicks_them() {
    let (url, kicks) = fake_mediamtx().await;
    let (app, state, _d) = app(&format!("relay = {{ enabled = true, api = \"{url}\" }}")).await;
    maio::relay::refresh(&state).await;
    assert_eq!(call(&app, "GET", "/api/v1/radios/main", OP, None).await.1["listeners"], 3);
    call(&app, "PATCH", "/api/v1/radios/main", ADMIN, Some(json!({"visibility": "private"}))).await;
    for _ in 0..50 {
        if kicks.lock().unwrap().len() == 3 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let mut k = kicks.lock().unwrap().clone();
    k.sort();
    assert_eq!(k, ["rtmp:r1", "rtsp:s1", "srt:c1"]);
}

#[tokio::test]
async fn a_new_key_on_a_public_radio_keeps_its_listeners() {
    let (app, state, _d) = app("serve = { mp3 = true }").await;
    let res = app.clone().oneshot(Request::get("/radios/main/stream.mp3").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(call(&app, "POST", "/api/v1/radios/main/listen-key", ADMIN, None).await.0, StatusCode::OK);
    let ended = tokio::time::timeout(Duration::from_millis(500), axum::body::to_bytes(res.into_body(), usize::MAX)).await;
    assert!(ended.is_err(), "the public address did not change, so the stream stays open");
    assert_eq!(state.main().listeners.load(std::sync::atomic::Ordering::Relaxed), 0, "the stream was dropped by the timeout above");
}

#[tokio::test]
async fn a_new_radio_can_start_with_a_playlist_on_air() {
    let (app, state, _d) = app("").await;
    state.store.insert_playlist(&common::playlist("pl_f", maio::playlists::Kind::Fixed, maio::playlists::Order::InOrder)).unwrap();
    assert_eq!(call(&app, "POST", "/api/v1/radios", ADMIN, Some(json!({"id": "lobby", "name": "Lobby", "playlist_id": "pl_nope"}))).await.0, StatusCode::NOT_FOUND);
    let (s, body) = call(&app, "POST", "/api/v1/radios", ADMIN, Some(json!({"id": "lobby", "name": "Lobby", "playlist_id": "pl_f"}))).await;
    assert_eq!((s, body["on_air"]["id"].as_str()), (StatusCode::CREATED, Some("pl_f")));
}

#[tokio::test]
async fn the_radio_list_knows_the_limit_and_which_radios_run() {
    let (app, _state, _d) = app("max_radios = 3").await;
    let (_, list) = call(&app, "GET", "/api/v1/radios", OP, None).await;
    assert_eq!((list["max_radios"].as_u64(), list["radios"][0]["running"].as_bool()), (Some(3), Some(true)));
}
