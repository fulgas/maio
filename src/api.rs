use std::{convert::Infallible, sync::Arc, time::Instant};

use axum::{
    Json, Router,
    extract::{FromRequest, FromRequestParts, Path, Query, State},
    http::{HeaderMap, Method, StatusCode, header, request::Parts},
    response::{Html, IntoResponse, Response, Sse, sse::{Event as SseEvent, KeepAlive}},
    routing::{delete, get, patch, post, put},
};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use tokio_stream::{StreamExt, wrappers::BroadcastStream};
use tower::ServiceExt;
use tower_http::services::ServeFile;

use crate::{
    accounts,
    auth::{Auth, LimitKind},
    config::Scope,
    engine,
    error::ApiError,
    jobs,
    playlists::{self, Kind, Order, Playlist, Recipe, Rule},
    playout::{self, PlayOrder},
    queue::{Entry, Position, RemoveError, Source},
    radio::{self, Radio},
    request::{Request as MusicRequest, Rules, normalize},
    state::{AppState, NowPlaying},
    storage,
    store::{self, RadioRow, Side, TrackFilter, Visibility},
    users::{self, principal},
};

pub const SESSION_COOKIE: &str = "maio_session";

fn cookie(parts: &Parts, name: &str) -> Option<String> {
    parts.headers.get_all(header::COOKIE).iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|kv| kv.trim().strip_prefix(name).and_then(|rest| rest.strip_prefix('=')).map(str::to_string))
}

/// True when the request's `Origin` names the host it was sent to (SPEC 8, cross-site protection).
pub fn same_origin(headers: &HeaderMap) -> bool {
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok());
    let origin = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok());
    match (host, origin.and_then(|o| o.split_once("://").map(|(_, h)| h))) {
        (Some(h), Some(o)) => h.eq_ignore_ascii_case(o.trim_end_matches('/')),
        _ => false,
    }
}

impl FromRequestParts<AppState> for Auth {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let id = cookie(parts, SESSION_COOKIE).ok_or_else(|| ApiError::unauthorized("sign in first"))?;
        let (user, session) = state.store.session_user(&id, chrono::Utc::now().timestamp())?
            .ok_or_else(|| ApiError::unauthorized("session ended; sign in again"))?;
        let changes_state = !matches!(parts.method, Method::GET | Method::HEAD);
        if changes_state && !same_origin(&parts.headers) {
            return Err(ApiError::forbidden("bad_origin", "request did not come from this site"));
        }
        if user.must_change {
            // Exact paths: inside the nested router the /api/v1 prefix is already stripped.
            let path = parts.uri.path().strip_prefix("/api/v1").unwrap_or(parts.uri.path());
            let allowed = matches!(
                (&parts.method, path),
                (&Method::GET, "/me") | (&Method::POST, "/me/password") | (&Method::DELETE, "/session")
            );
            if !allowed {
                return Err(ApiError::forbidden("password_change_required", "choose a new password first"));
            }
        }
        Ok(Auth { principal: principal(&user, &state.limits), user, session: session.id })
    }
}

/// JSON body whose rejections use the API error format.
pub struct Body<T>(pub T);

impl<T: DeserializeOwned, S: Send + Sync> FromRequest<S> for Body<T> {
    type Rejection = ApiError;

    async fn from_request(req: axum::extract::Request, state: &S) -> Result<Self, ApiError> {
        Json::<T>::from_request(req, state).await.map(|Json(v)| Body(v)).map_err(|e| ApiError::invalid("invalid_body", e.body_text()))
    }
}

/// The radio a request acts on: `/radios/{radio}/…`, or `main` on the old unprefixed paths (SPEC 10).
pub struct OnRadio(pub Arc<Radio>);

impl FromRequestParts<AppState> for OnRadio {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        // Nested routers strip their prefix from `uri`; the original keeps `/radios/{radio}`.
        let uri = parts.extensions.get::<axum::extract::OriginalUri>().map_or(&parts.uri, |o| &o.0);
        let path = uri.path();
        let path = path.strip_prefix("/api/v1").unwrap_or(path);
        let id = path.strip_prefix("/radios/").and_then(|rest| rest.split('/').next()).unwrap_or("main");
        state.radio(id).map(OnRadio).ok_or_else(|| ApiError::not_found("no such radio"))
    }
}

pub fn router(state: AppState) -> Router {
    let per_radio = || Router::new()
        .route("/status", get(status))
        .route("/events", get(events))
        .route("/queue", get(get_queue).post(post_queue).delete(clear_queue))
        .route("/queue/{entry_id}", delete(delete_entry))
        .route("/skip", post(skip))
        .route("/history", get(history))
        .route("/on-air", get(get_on_air).put(set_on_air))
        .route("/start", post(start))
        .route("/stop", post(stop));
    let api = Router::new()
        .merge(per_radio())
        .nest("/radios/{radio}", per_radio())
        .route("/radios", get(list_radios).post(create_radio))
        .route("/radios/{radio}", get(get_radio).patch(patch_radio).delete(delete_radio))
        .route("/radios/{radio}/listen-key", post(new_listen_key))
        .route("/playout/start", post(start))
        .route("/playout/stop", post(stop))
        .route("/session", post(accounts::sign_in).delete(accounts::sign_out))
        .route("/me", get(accounts::me))
        .route("/me/password", post(accounts::change_password))
        .route("/users", get(accounts::list_users).post(accounts::create_user))
        .route("/users/{username}", patch(accounts::patch_user).delete(accounts::delete_user))
        .route("/users/{username}/password", post(accounts::reset_password))
        .route("/generate", post(generate))
        .route("/jobs/{id}", get(get_job))
        .route("/library", get(library).delete(empty_crate))
        .route("/library/{id}", get(track).delete(delete_track).patch(rename_track))
        .route("/library/{id}/audio", get(track_audio))
        .route("/library/{id}/favourite", put(favourite))
        .route("/engines", get(engines))
        .route("/playlists", get(list_playlists).post(create_playlist))
        .route("/playlists/{id}", get(get_playlist).patch(patch_playlist).delete(delete_playlist))
        .route("/playlists/{id}/tracks", post(add_playlist_tracks).put(set_playlist_tracks))
        .route("/playlists/{id}/tracks/{track_id}", delete(remove_playlist_track));
    let mut app = Router::new()
        .nest("/api/v1", api)
        .route("/", get(|| async { Html(include_str!("../web/index.html")) }))
        .route("/app.css", get(|| async { asset("text/css; charset=utf-8", include_str!("../web/app.css")) }))
        .route("/app.js", get(|| async { asset("text/javascript; charset=utf-8", include_str!("../web/app.js")) }))
        .route("/ui.js", get(|| async { asset("text/javascript; charset=utf-8", include_str!("../web/ui.js")) }))
        .route("/relay/auth", post(relay_auth));
    if state.cfg.serve.mp3 {
        app = app
            .route("/stream.mp3", get(|State(s): State<AppState>, headers: HeaderMap| async move { stream_of(public_radio(&s, "main"), Format::Mp3, wants_icy(&headers)) }))
            .route("/radios/{radio}/stream.mp3", get(|State(s): State<AppState>, Path(id): Path<String>, headers: HeaderMap| async move { stream_of(public_radio(&s, &id), Format::Mp3, wants_icy(&headers)) }))
            .route("/listen/{key}/stream.mp3", get(|State(s): State<AppState>, Path(k): Path<String>, headers: HeaderMap| async move { stream_of(private_radio(&s, &k), Format::Mp3, wants_icy(&headers)) }));
    }
    if state.cfg.serve.aac {
        app = app
            .route("/stream.aac", get(|State(s): State<AppState>, headers: HeaderMap| async move { stream_of(public_radio(&s, "main"), Format::Aac, wants_icy(&headers)) }))
            .route("/radios/{radio}/stream.aac", get(|State(s): State<AppState>, Path(id): Path<String>, headers: HeaderMap| async move { stream_of(public_radio(&s, &id), Format::Aac, wants_icy(&headers)) }))
            .route("/listen/{key}/stream.aac", get(|State(s): State<AppState>, Path(k): Path<String>, headers: HeaderMap| async move { stream_of(private_radio(&s, &k), Format::Aac, wants_icy(&headers)) }));
    }
    if state.cfg.serve.hls {
        app = app
            .route("/hls/{file}", get(|State(s): State<AppState>, Path(f): Path<String>, req: axum::extract::Request| async move {
                hls_file(&s, public_radio(&s, "main"), &f, req).await
            }))
            .route("/radios/{radio}/hls/{file}", get(|State(s): State<AppState>, Path((id, f)): Path<(String, String)>, req: axum::extract::Request| async move {
                hls_file(&s, public_radio(&s, &id), &f, req).await
            }))
            .route("/listen/{key}/hls/{file}", get(|State(s): State<AppState>, Path((k, f)): Path<(String, String)>, req: axum::extract::Request| async move {
                hls_file(&s, private_radio(&s, &k), &f, req).await
            }));
    }
    app.with_state(state)
}

pub fn limit(s: &AppState, auth: &Auth, kind: LimitKind) -> Result<(), ApiError> {
    s.limiter.check(&auth.principal, kind, Instant::now())
}

fn rules<'a>(s: &'a AppState, auth: &'a Auth, ids: &'a [String]) -> Rules<'a> {
    Rules { moods: &s.cfg.moods, genres: &s.cfg.genres, scopes: &auth.principal.scopes, playlist_name: None,
            text_engines: ids, default_language: &s.cfg.lyrics.default_language }
}

fn pending_ok(s: &AppState, auth: &Auth) -> Result<(), ApiError> {
    if jobs::pending_for(s, auth.name()) >= auth.principal.limits.pending_jobs {
        return Err(ApiError::too_many_jobs());
    }
    Ok(())
}

/// New music needs space; a crate full of kept tracks refuses it (SPEC 6, Storage).
fn room_for(s: &AppState, seconds: u32) -> Result<(), ApiError> {
    if storage::make_room(s, storage::estimate_bytes(s, seconds), &[]) {
        Ok(())
    } else {
        Err(ApiError::new(StatusCode::INSUFFICIENT_STORAGE, "library_full", "the crate is full of kept tracks; empty it or stop keeping some"))
    }
}

fn engines_json(s: &AppState) -> Value {
    Value::Array(s.engines.iter().map(|e| {
        let (info, health) = e.snapshot();
        json!({"id": e.cfg.id, "type": "music", "url": e.cfg.url, "health": health, "info": info})
    }).collect())
}

async fn status(State(s): State<AppState>, auth: Auth, OnRadio(r): OnRadio) -> Result<Json<Value>, ApiError> {
    auth.require(Scope::Read)?;
    let (stopped, now) = {
        let p = r.playout.lock().unwrap();
        (p.stopped, p.now.as_ref().map(NowPlaying::json))
    };
    let row = r.row.lock().unwrap().clone();
    let outputs = json!(&*r.outputs.lock().unwrap());
    Ok(Json(json!({
        "radio": {"id": row.id, "name": row.name, "visibility": row.visibility},
        "playout": {"running": !stopped},
        "now_playing": now,
        "queue": s.queue_json(&r),
        "on_air": on_air_brief(&s, &r),
        "engines": engines_json(&s),
        "outputs": outputs,
        "storage": storage::status(&s),
        "moods": s.cfg.moods,
        "genres": s.cfg.genres,
    })))
}

async fn events(State(s): State<AppState>, auth: Auth, OnRadio(r): OnRadio) -> Result<impl IntoResponse, ApiError> {
    auth.require(Scope::Read)?;
    let id = r.id.clone();
    let stream = BroadcastStream::new(s.events.subscribe()).filter_map(move |e| {
        let e = e.ok()?;
        (e.radio.is_none() || e.radio.as_deref() == Some(id.as_str()))
            .then(|| Ok::<_, Infallible>(SseEvent::default().event(e.kind).data(e.data.to_string())))
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

async fn get_queue(State(s): State<AppState>, auth: Auth, OnRadio(r): OnRadio) -> Result<Json<Value>, ApiError> {
    auth.require(Scope::Read)?;
    Ok(Json(s.queue_json(&r)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QueueBody {
    track_id: Option<String>,
    request: Option<MusicRequest>,
    #[serde(default)]
    position: Position,
}

async fn post_queue(State(s): State<AppState>, auth: Auth, OnRadio(r): OnRadio, Body(body): Body<QueueBody>) -> Result<(StatusCode, Json<Value>), ApiError> {
    auth.require(Scope::Queue)?;
    limit(&s, &auth, LimitKind::Queue)?;
    match (body.track_id, body.request) {
        (Some(track_id), None) => {
            let track = s.store.track(&track_id)?.ok_or_else(|| ApiError::not_found("no such track"))?;
            let entry = Entry::ready(jobs::new_id("q"), &track, Source::Manual, auth.name());
            let id = entry.id.clone();
            r.queue.lock().unwrap().add(entry, body.position);
            s.emit_queue(&r);
            Ok((StatusCode::ACCEPTED, Json(json!({"entry_id": id, "state": "ready"}))))
        }
        (None, Some(req)) => {
            let ids = s.cfg.text_engine_ids();
            let n = normalize(&req, &rules(&s, &auth, &ids))?;
            engine::choose(&s.engines, &n, &[])?;
            room_for(&s, n.seconds)?;
            pending_ok(&s, &auth)?;
            let (entry_id, job_id) = (jobs::new_id("q"), jobs::new_id("job"));
            let eta = jobs::eta_s(&s, &n);
            let entry = Entry::generating(entry_id.clone(), job_id.clone(), &n, Source::Manual, auth.name(), None, eta);
            r.queue.lock().unwrap().add(entry, body.position);
            jobs::start(&s, job_id.clone(), n, auth.name(), Some(&r.id), Some(entry_id.clone()), None);
            s.emit_queue(&r);
            Ok((StatusCode::ACCEPTED, Json(json!({"entry_id": entry_id, "state": "generating", "job_id": job_id, "eta_s": eta}))))
        }
        _ => Err(ApiError::invalid("invalid_body", "send track_id or request")),
    }
}

async fn delete_entry(State(s): State<AppState>, auth: Auth, OnRadio(r): OnRadio, Path(params): Path<std::collections::HashMap<String, String>>) -> Result<StatusCode, ApiError> {
    auth.require(Scope::Queue)?;
    limit(&s, &auth, LimitKind::Write)?;
    let id = params.get("entry_id").cloned().unwrap_or_default();
    let removed = r.queue.lock().unwrap().remove(&id);
    match removed {
        Ok(job) => {
            jobs::cancel(&s, &job.into_iter().collect::<Vec<_>>());
            s.emit_queue(&r);
            Ok(StatusCode::NO_CONTENT)
        }
        Err(RemoveError::NotFound) => Err(ApiError::not_found("no such entry")),
        Err(RemoveError::Started) => Err(ApiError::conflict("entry_started", "the entry has started or is about to")),
    }
}

#[derive(Deserialize)]
struct ClearQuery {
    side: Option<String>,
}

/// Clears a side of the queue (SPEC 3). Side A (default): what people queued.
/// Side B: the playlist's tracks, and the playlist goes off air so they do not come straight back.
async fn clear_queue(State(s): State<AppState>, auth: Auth, OnRadio(r): OnRadio, Query(q): Query<ClearQuery>) -> Result<StatusCode, ApiError> {
    auth.require(Scope::Queue)?;
    let side_b = match q.side.as_deref() {
        None | Some("a") => false,
        Some("b") => true,
        Some(_) => return Err(ApiError::invalid("bad_side", "side is a or b")),
    };
    if side_b {
        auth.require(Scope::Control)?;
    }
    limit(&s, &auth, LimitKind::Write)?;
    let cancels = r.queue.lock().unwrap().replace(|e| e.filler == side_b);
    jobs::cancel(&s, &cancels);
    if side_b && s.store.on_air(&r.id)?.is_some() {
        s.store.set_on_air(&r.id, None)?;
        s.emit(Some(&r.id), "on_air", Value::Null);
    }
    s.emit_queue(&r);
    Ok(StatusCode::NO_CONTENT)
}

/// Deletes one track and its file; a queued or playing track is refused.
async fn delete_track(State(s): State<AppState>, auth: Auth, Path(id): Path<String>) -> Result<StatusCode, ApiError> {
    auth.require(Scope::Queue)?;
    limit(&s, &auth, LimitKind::Write)?;
    let t = s.store.track(&id)?.ok_or_else(|| ApiError::not_found("no such track"))?;
    if s.busy_track_ids().contains(&id) {
        return Err(ApiError::conflict("track_in_use", "this track is queued or playing"));
    }
    let _ = tokio::fs::remove_file(&t.file).await;
    s.store.mark_deleted(&id)?;
    tracing::info!(actor = %auth.name(), track = %id, "track deleted");
    Ok(StatusCode::NO_CONTENT)
}

/// Empties the crate: deletes every track that is not kept, queued or playing (admins only).
async fn empty_crate(State(s): State<AppState>, auth: Auth) -> Result<Json<Value>, ApiError> {
    auth.require(Scope::Admin)?;
    limit(&s, &auth, LimitKind::Write)?;
    let keep = s.busy_track_ids();
    let doomed = s.store.unkept(&keep)?;
    for (id, file) in &doomed {
        let _ = tokio::fs::remove_file(file).await;
        s.store.mark_deleted(id)?;
    }
    tracing::info!(actor = %auth.name(), count = doomed.len(), "crate emptied");
    Ok(Json(json!({"deleted": doomed.len()})))
}

async fn skip(State(s): State<AppState>, auth: Auth, OnRadio(r): OnRadio) -> Result<StatusCode, ApiError> {
    auth.require(Scope::Control)?;
    limit(&s, &auth, LimitKind::Write)?;
    r.playout.lock().unwrap().skip = true;
    Ok(StatusCode::ACCEPTED)
}

async fn generate(State(s): State<AppState>, auth: Auth, Body(req): Body<MusicRequest>) -> Result<(StatusCode, Json<Value>), ApiError> {
    auth.require(Scope::Queue)?;
    limit(&s, &auth, LimitKind::Generate)?;
    pending_ok(&s, &auth)?;
    let ids = s.cfg.text_engine_ids();
    let n = normalize(&req, &rules(&s, &auth, &ids))?;
    engine::choose(&s.engines, &n, &[])?;
    room_for(&s, n.seconds)?;
    let job_id = jobs::new_id("job");
    let eta = jobs::eta_s(&s, &n);
    jobs::start(&s, job_id.clone(), n, auth.name(), None, None, None);
    Ok((StatusCode::ACCEPTED, Json(json!({"job_id": job_id, "eta_s": eta}))))
}

async fn get_job(State(s): State<AppState>, auth: Auth, Path(id): Path<String>) -> Result<Json<Value>, ApiError> {
    auth.require(Scope::Read)?;
    let job = s.jobs.lock().unwrap().get(&id).cloned().ok_or_else(|| ApiError::not_found("no such job"))?;
    Ok(Json(json!(job)))
}

#[derive(Deserialize)]
struct LibraryQuery {
    q: Option<String>,
    mood: Option<String>,
    side: Option<String>,
    playlist: Option<String>,
    fav: Option<bool>,
    limit: Option<u32>,
    cursor: Option<u32>,
    sort: Option<String>,
}

async fn library(State(s): State<AppState>, auth: Auth, Query(q): Query<LibraryQuery>) -> Result<Json<Value>, ApiError> {
    auth.require(Scope::Read)?;
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let side = match q.side.as_deref() {
        None | Some("") => None,
        Some("a") => Some(Side::A),
        Some("b") => Some(Side::B),
        Some(_) => return Err(ApiError::invalid("bad_side", "side is a or b")),
    };
    let pl = match q.playlist.as_deref().filter(|p| !p.is_empty()) {
        Some(id) => Some(find_playlist(&s, id)?),
        None => None,
    };
    let filter = TrackFilter { q: q.q.as_deref(), mood: q.mood.as_deref(), fav: q.fav, side, playlist: pl.as_ref() };
    let sort = match q.sort.as_deref() {
        None | Some("") | Some("newest") => store::Sort::Newest,
        Some("played") => store::Sort::Played,
        Some("title") => store::Sort::Title,
        Some(_) => return Err(ApiError::invalid("bad_sort", "sort is newest, played or title")),
    };
    let (tracks, next) = s.store.tracks(&filter, sort, limit, q.cursor.unwrap_or(0))?;
    let last = s.store.last_played(&tracks.iter().map(|t| t.id.clone()).collect::<Vec<_>>())?;
    let tracks: Vec<Value> = tracks.into_iter().map(|t| {
        let mut v = json!(t);
        v["last_played_at"] = json!(last.get(&t.id));
        v
    }).collect();
    Ok(Json(json!({"tracks": tracks, "next_cursor": next})))
}

async fn track(State(s): State<AppState>, auth: Auth, Path(id): Path<String>) -> Result<Json<Value>, ApiError> {
    auth.require(Scope::Read)?;
    let t = s.store.track(&id)?.ok_or_else(|| ApiError::not_found("no such track"))?;
    Ok(Json(json!(t)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rename {
    title: String,
}

/// A person names a track (SPEC 10, Titles): `title_source = "person"`, never overwritten.
async fn rename_track(State(s): State<AppState>, auth: Auth, Path(id): Path<String>, Body(b): Body<Rename>) -> Result<Json<Value>, ApiError> {
    auth.require(Scope::Queue)?;
    limit(&s, &auth, LimitKind::Write)?;
    let title = b.title.trim();
    if title.is_empty() || title.chars().count() > 80 || title.chars().any(char::is_control) {
        return Err(ApiError::invalid("bad_title", "a title is 1 to 80 characters on one line"));
    }
    if !s.store.rename_track(&id, title)? {
        return Err(ApiError::not_found("no such track"));
    }
    let t = s.store.track(&id)?.ok_or_else(|| ApiError::not_found("no such track"))?;
    let file = std::path::Path::new(&t.file);
    match crate::media::write_tags(file, &crate::media::Tags::from(&t)).await {
        Ok(()) => {
            if let Ok(m) = tokio::fs::metadata(file).await {
                s.store.set_bytes(&t.id, m.len() as i64)?;
            }
        }
        Err(e) => tracing::warn!(track = %t.id, "rewriting tags after a rename: {e:#}"),
    }
    let t = s.store.track(&id)?.unwrap_or(t);
    Ok(Json(json!(t)))
}

async fn track_audio(State(s): State<AppState>, auth: Auth, Path(id): Path<String>, req: axum::extract::Request) -> Result<Response, ApiError> {
    auth.require(Scope::Read)?;
    let t = s.store.track(&id)?.ok_or_else(|| ApiError::not_found("no such track"))?;
    match ServeFile::new(&t.file).oneshot(req).await {
        Ok(res) => Ok(res.into_response()),
        Err(never) => match never {},
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FavBody {
    favourite: bool,
}

async fn favourite(State(s): State<AppState>, auth: Auth, Path(id): Path<String>, Body(b): Body<FavBody>) -> Result<StatusCode, ApiError> {
    auth.require(Scope::Control)?;
    limit(&s, &auth, LimitKind::Write)?;
    if !s.store.set_favourite(&id, b.favourite)? {
        return Err(ApiError::not_found("no such track"));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct Page {
    limit: Option<u32>,
    cursor: Option<i64>,
}

async fn history(State(s): State<AppState>, auth: Auth, OnRadio(r): OnRadio, Query(p): Query<Page>) -> Result<Json<Value>, ApiError> {
    auth.require(Scope::Read)?;
    let (plays, next) = s.store.history(&r.id, p.limit.unwrap_or(50).clamp(1, 200), p.cursor)?;
    Ok(Json(json!({"plays": plays, "next_cursor": next})))
}

/// A playlist as the API shows it: the size limit in force and how many tracks it plays.
pub fn playlist_json(s: &AppState, p: &Playlist) -> Value {
    let max = s.cfg.playlists.max_tracks;
    let mut v = json!(p);
    v["max_tracks"] = json!(p.limit(max));
    v["count"] = json!(s.store.member_count(p, p.cap(max)).unwrap_or(0));
    v["seconds"] = json!(s.store.member_seconds(p, p.cap(max)).unwrap_or(0.0));
    v
}

fn on_air_brief(s: &AppState, r: &Radio) -> Value {
    playout::on_air(s, r).map_or(Value::Null, |p| json!({"id": p.id, "name": p.name, "kind": p.kind}))
}

/// The playlist on air on `r`, the track playing now and the next ones it will play (SPEC 10), or `null`.
fn on_air_json(s: &AppState, r: &Radio) -> Value {
    let Some(pl) = playout::on_air(s, r) else { return Value::Null };
    let now = r.playout.lock().unwrap().now.as_ref().map(|n| n.track.id.clone());
    json!({"playlist": playlist_json(s, &pl), "now_track_id": now, "upcoming": playout::upcoming(s, r, &pl, 5)})
}

/// Edits to a playlist on air take effect at once: every radio playing it draws its order again.
fn redraw(s: &AppState, id: &str) {
    for r in s.radios() {
        let mut po = r.play_order.lock().unwrap();
        if po.playlist.as_deref() == Some(id) {
            *po = PlayOrder::default();
        }
    }
}

fn playlists_event(s: &AppState) {
    s.emit(None, "playlists", Value::Null);
}

fn find_playlist(s: &AppState, id: &str) -> Result<Playlist, ApiError> {
    playlists::find(&s.store, id)?.ok_or_else(|| ApiError::not_found("no such playlist"))
}

/// Playlists people own; built-ins are never edited.
fn editable(s: &AppState, id: &str) -> Result<Playlist, ApiError> {
    let p = find_playlist(s, id)?;
    if p.kind.builtin() {
        return Err(ApiError::conflict("builtin_playlist", "built-in playlists cannot be changed"));
    }
    Ok(p)
}

/// Playlists whose tracks people add, remove and reorder (fixed and generative).
fn fillable(s: &AppState, id: &str) -> Result<Playlist, ApiError> {
    let p = find_playlist(s, id)?;
    if !p.kind.stored() {
        return Err(ApiError::conflict("playlist_not_editable", "only fixed and generative playlists take tracks by hand"));
    }
    Ok(p)
}

/// Checks a playlist before it is stored (SPEC 10): name, size, and the recipe or rule its kind needs.
/// `new_rule`: the rule was just sent, so `from_playlist` must exist; a stored one may point at a deleted playlist.
fn check_playlist(s: &AppState, p: &Playlist, new_rule: bool) -> Result<(), ApiError> {
    radio_name(&p.name)?;
    if p.max_tracks.is_some_and(|n| n == 0 || n > s.cfg.playlists.max_tracks_limit) {
        return Err(ApiError::invalid("bad_max_tracks", format!("max_tracks must be 1 to {}", s.cfg.playlists.max_tracks_limit)));
    }
    match (p.kind, &p.recipe, &p.rule) {
        (Kind::Generative, Some(r), None) => r.validate(&p.name, &s.cfg),
        (Kind::Smart, None, Some(rule)) => {
            rule.validate(&s.cfg.moods, &s.cfg.genres)?;
            if let Some(from) = rule.from_playlist.as_ref().filter(|_| new_rule) {
                if !s.store.playlist(from)?.is_some_and(|f| f.kind.stored()) {
                    return Err(ApiError::invalid("unknown_playlist", "from_playlist must name a fixed or generative playlist"));
                }
            }
            Ok(())
        }
        (Kind::Fixed, None, None) => Ok(()),
        _ => Err(ApiError::invalid("bad_playlist", "generative playlists need a recipe, smart ones a rule, fixed ones neither")),
    }
}

async fn list_playlists(State(s): State<AppState>, auth: Auth) -> Result<Json<Value>, ApiError> {
    auth.require(Scope::Read)?;
    let builtins = playlists::BUILTINS.iter().filter_map(|id| Playlist::builtin(id));
    let all: Vec<Value> = builtins.chain(s.store.playlists()?).map(|p| playlist_json(&s, &p)).collect();
    Ok(Json(json!({"playlists": all})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewPlaylist {
    name: String,
    kind: Kind,
    order: Option<Order>,
    max_tracks: Option<u32>,
    recipe: Option<Recipe>,
    rule: Option<Rule>,
}

async fn create_playlist(State(s): State<AppState>, auth: Auth, Body(b): Body<NewPlaylist>) -> Result<(StatusCode, Json<Value>), ApiError> {
    auth.require(Scope::Queue)?;
    limit(&s, &auth, LimitKind::Write)?;
    if b.kind.builtin() {
        return Err(ApiError::invalid("bad_kind", "kind is fixed, generative or smart"));
    }
    let p = Playlist {
        id: jobs::new_id("pl"),
        name: b.name.trim().to_string(),
        kind: b.kind,
        order: b.order.unwrap_or(if b.kind == Kind::Fixed { Order::InOrder } else { Order::Shuffle }),
        max_tracks: b.max_tracks,
        recipe: b.recipe,
        rule: if b.kind == Kind::Smart { Some(b.rule.unwrap_or_default()) } else { b.rule },
        created_at: store::now(),
    };
    check_playlist(&s, &p, true)?;
    s.store.insert_playlist(&p)?;
    tracing::info!(actor = %auth.name(), playlist = %p.id, kind = p.kind.as_str(), "playlist created");
    playlists_event(&s);
    Ok((StatusCode::CREATED, Json(playlist_json(&s, &p))))
}

#[derive(Deserialize)]
struct PlaylistPage {
    limit: Option<u32>,
    cursor: Option<u32>,
}

async fn get_playlist(State(s): State<AppState>, auth: Auth, Path(id): Path<String>, Query(q): Query<PlaylistPage>) -> Result<Json<Value>, ApiError> {
    auth.require(Scope::Read)?;
    let p = find_playlist(&s, &id)?;
    let (limit, offset) = (q.limit.unwrap_or(50).clamp(1, 200), q.cursor.unwrap_or(0));
    let tracks = s.store.playlist_tracks(&p, p.cap(s.cfg.playlists.max_tracks), limit, offset)?;
    let next = (tracks.len() as u32 == limit).then(|| offset + limit);
    let mut v = playlist_json(&s, &p);
    v["tracks"] = json!(tracks);
    v["next_cursor"] = json!(next);
    Ok(Json(v))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PatchPlaylist {
    name: Option<String>,
    order: Option<Order>,
    max_tracks: Option<u32>,
    recipe: Option<Recipe>,
    rule: Option<Rule>,
}

async fn patch_playlist(State(s): State<AppState>, auth: Auth, Path(id): Path<String>, Body(b): Body<PatchPlaylist>) -> Result<Json<Value>, ApiError> {
    auth.require(Scope::Queue)?;
    limit(&s, &auth, LimitKind::Write)?;
    let mut p = editable(&s, &id)?;
    if let Some(n) = b.name {
        p.name = n.trim().to_string();
    }
    p.order = b.order.unwrap_or(p.order);
    p.max_tracks = b.max_tracks.or(p.max_tracks);
    p.recipe = b.recipe.or(p.recipe);
    let new_rule = b.rule.is_some();
    p.rule = b.rule.or(p.rule);
    check_playlist(&s, &p, new_rule)?;
    s.store.update_playlist(&p)?;
    redraw(&s, &p.id);
    playlists_event(&s);
    Ok(Json(playlist_json(&s, &p)))
}

async fn delete_playlist(State(s): State<AppState>, auth: Auth, Path(id): Path<String>) -> Result<StatusCode, ApiError> {
    auth.require(Scope::Queue)?;
    limit(&s, &auth, LimitKind::Write)?;
    editable(&s, &id)?;
    if s.radios().iter().any(|r| s.store.on_air(&r.id).ok().flatten().as_deref() == Some(id.as_str())) {
        return Err(ApiError::conflict("playlist_on_air", "take this playlist off air on every radio before deleting it"));
    }
    if !s.store.delete_playlist(&id)? {
        return Err(ApiError::not_found("no such playlist"));
    }
    tracing::info!(actor = %auth.name(), playlist = %id, "playlist deleted");
    playlists_event(&s);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AddTracks {
    track_ids: Vec<String>,
    position: Option<usize>,
}

fn known_tracks(s: &AppState, ids: &[String]) -> Result<(), ApiError> {
    for id in ids {
        if s.store.track(id)?.is_none() {
            return Err(ApiError::invalid("unknown_track", format!("no track {id:?} in the crate")));
        }
    }
    Ok(())
}

/// Refuses growing past the size; a playlist already over it (for example after a migration) can still shrink or reorder.
fn full(p: &Playlist, s: &AppState, n: usize, current: usize) -> Result<(), ApiError> {
    match p.limit(s.cfg.playlists.max_tracks) {
        Some(max) if n > max as usize && n > current => Err(ApiError::conflict("playlist_full", format!("this playlist holds at most {max} tracks"))),
        _ => Ok(()),
    }
}

async fn add_playlist_tracks(State(s): State<AppState>, auth: Auth, Path(id): Path<String>, Body(b): Body<AddTracks>) -> Result<Json<Value>, ApiError> {
    auth.require(Scope::Queue)?;
    limit(&s, &auth, LimitKind::Write)?;
    let p = fillable(&s, &id)?;
    known_tracks(&s, &b.track_ids)?;
    let current: std::collections::HashSet<String> = s.store.member_ids(&p, None)?.into_iter().collect();
    let new: std::collections::HashSet<&String> = b.track_ids.iter().filter(|t| !current.contains(*t)).collect();
    full(&p, &s, current.len() + new.len(), current.len())?;
    s.store.add_to_playlist(&id, &b.track_ids, b.position)?;
    redraw(&s, &id);
    playlists_event(&s);
    Ok(Json(playlist_json(&s, &p)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetTracks {
    track_ids: Vec<String>,
}

async fn set_playlist_tracks(State(s): State<AppState>, auth: Auth, Path(id): Path<String>, Body(b): Body<SetTracks>) -> Result<Json<Value>, ApiError> {
    auth.require(Scope::Queue)?;
    limit(&s, &auth, LimitKind::Write)?;
    let p = fillable(&s, &id)?;
    if b.track_ids.iter().collect::<std::collections::HashSet<_>>().len() != b.track_ids.len() {
        return Err(ApiError::invalid("duplicate_track", "a track appears once in a playlist"));
    }
    known_tracks(&s, &b.track_ids)?;
    full(&p, &s, b.track_ids.len(), s.store.member_count(&p, None)? as usize)?;
    s.store.set_playlist_tracks(&id, &b.track_ids)?;
    redraw(&s, &id);
    playlists_event(&s);
    Ok(Json(playlist_json(&s, &p)))
}

async fn remove_playlist_track(State(s): State<AppState>, auth: Auth, Path((id, track_id)): Path<(String, String)>) -> Result<StatusCode, ApiError> {
    auth.require(Scope::Queue)?;
    limit(&s, &auth, LimitKind::Write)?;
    fillable(&s, &id)?;
    if !s.store.remove_from_playlist(&id, &track_id)? {
        return Err(ApiError::not_found("that track is not in this playlist"));
    }
    redraw(&s, &id);
    playlists_event(&s);
    Ok(StatusCode::NO_CONTENT)
}

async fn get_on_air(State(s): State<AppState>, auth: Auth, OnRadio(r): OnRadio) -> Result<Json<Value>, ApiError> {
    auth.require(Scope::Read)?;
    Ok(Json(on_air_json(&s, &r)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetOnAir {
    playlist_id: Option<String>,
}

/// Puts a playlist on air on this radio, or none (SPEC 10). Side B from before the switch is replaced.
async fn set_on_air(State(s): State<AppState>, auth: Auth, OnRadio(r): OnRadio, Body(b): Body<SetOnAir>) -> Result<Json<Value>, ApiError> {
    auth.require(Scope::Control)?;
    limit(&s, &auth, LimitKind::Station)?;
    if let Some(id) = &b.playlist_id {
        playlists::find(&s.store, id)?.ok_or_else(|| ApiError::not_found("no such playlist"))?;
    }
    s.store.set_on_air(&r.id, b.playlist_id.as_deref())?;
    *r.play_order.lock().unwrap() = PlayOrder::default();
    let cancels = r.queue.lock().unwrap().replace(|e| e.source == Source::Playlist || e.filler);
    jobs::cancel(&s, &cancels);
    s.emit(Some(&r.id), "on_air", on_air_brief(&s, &r));
    s.emit_queue(&r);
    radios_event(&s);
    Ok(Json(on_air_json(&s, &r)))
}

async fn engines(State(s): State<AppState>, auth: Auth) -> Result<Json<Value>, ApiError> {
    auth.require(Scope::Read)?;
    let mut list = engines_json(&s).as_array().cloned().unwrap_or_default();
    // ACE-Step writes lyrics itself (SPEC 10): a lyrics-only text engine wherever an ACE-Step engine is.
    for e in s.engines.iter().filter(|e| e.cfg.id == "ace-step" || e.snapshot().0.is_some_and(|i| i.id == "ace-step")) {
        list.push(json!({"id": "ace-step", "type": "text", "kind": "ace-step", "licence": e.snapshot().0.map(|i| i.licence.name), "health": e.snapshot().1}));
    }
    for t in &s.text_engines {
        list.push(json!({"id": t.cfg.id, "type": "text", "kind": t.cfg.kind, "model": t.cfg.model, "licence": t.licence(), "health": t.health()}));
    }
    Ok(Json(Value::Array(list)))
}

async fn start(State(s): State<AppState>, auth: Auth, OnRadio(r): OnRadio) -> Result<StatusCode, ApiError> {
    auth.require(Scope::Admin)?;
    limit(&s, &auth, LimitKind::Write)?;
    r.playout.lock().unwrap().stopped = false;
    Ok(StatusCode::ACCEPTED)
}

async fn stop(State(s): State<AppState>, auth: Auth, OnRadio(r): OnRadio) -> Result<StatusCode, ApiError> {
    auth.require(Scope::Admin)?;
    limit(&s, &auth, LimitKind::Write)?;
    r.playout.lock().unwrap().stopped = true;
    Ok(StatusCode::ACCEPTED)
}

/// MediaMTX's HTTP auth hook (SPEC 7). `401` also makes RTSP clients send their credentials.
async fn relay_auth(State(s): State<AppState>, Json(req): Json<crate::relay::AuthRequest>) -> StatusCode {
    if s.cfg.relay.enabled && crate::relay::authorize(&s, &req) { StatusCode::OK } else { StatusCode::UNAUTHORIZED }
}

fn radio_json(s: &AppState, r: &Radio) -> Value {
    let row = r.row.lock().unwrap().clone();
    let (now, running) = {
        let p = r.playout.lock().unwrap();
        (p.now.as_ref().map(NowPlaying::json), !p.stopped)
    };
    json!({
        "running": running,
        "id": row.id, "name": row.name, "visibility": row.visibility,
        "on_air": on_air_brief(s, r), "now_playing": now,
        "listeners": r.listeners.load(std::sync::atomic::Ordering::Relaxed)
            + s.relay_readers.lock().unwrap().get(&crate::relay::path(&row)).copied().unwrap_or(0),
        "outputs": &*r.outputs.lock().unwrap(),
        "listen": listen_json(s, r),
    })
}

fn radios_event(s: &AppState) {
    let list: Vec<Value> = s.radios().iter().map(|r| radio_json(s, r)).collect();
    s.emit(None, "radios", Value::Array(list));
}

async fn list_radios(State(s): State<AppState>, auth: Auth) -> Result<Json<Value>, ApiError> {
    auth.require(Scope::Read)?;
    Ok(Json(json!({"radios": s.radios().iter().map(|r| radio_json(&s, r)).collect::<Vec<_>>(), "max_radios": s.cfg.max_radios})))
}

async fn get_radio(State(s): State<AppState>, auth: Auth, Path(id): Path<String>) -> Result<Json<Value>, ApiError> {
    auth.require(Scope::Read)?;
    let r = s.radio(&id).ok_or_else(|| ApiError::not_found("no such radio"))?;
    Ok(Json(radio_json(&s, &r)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NewRadio {
    id: String,
    name: String,
    visibility: Option<Visibility>,
    playlist_id: Option<String>,
}

fn radio_name(n: &str) -> Result<String, ApiError> {
    let n = n.trim();
    if n.is_empty() || n.chars().count() > 80 {
        return Err(ApiError::invalid("bad_name", "name must be 1 to 80 characters"));
    }
    Ok(n.to_string())
}

async fn create_radio(State(s): State<AppState>, auth: Auth, Body(b): Body<NewRadio>) -> Result<(StatusCode, Json<Value>), ApiError> {
    auth.require(Scope::Admin)?;
    limit(&s, &auth, LimitKind::Write)?;
    if !radio::valid_id(&b.id) {
        return Err(ApiError::invalid("bad_radio_id", "radio ids are 2 to 32 characters from a-z 0-9 -"));
    }
    let name = radio_name(&b.name)?;
    if let Some(p) = &b.playlist_id {
        playlists::find(&s.store, p)?.ok_or_else(|| ApiError::not_found("no such playlist"))?;
    }
    let r = {
        let mut radios = s.radios.lock().unwrap(); // held across the checks and the insert, so two creates cannot both pass
        if radios.iter().any(|r| r.id == b.id) {
            return Err(ApiError::conflict("radio_exists", "that radio id is taken"));
        }
        if radios.len() >= s.cfg.max_radios {
            return Err(ApiError::conflict("radio_limit", format!("at the limit of {} radios (max_radios)", s.cfg.max_radios)));
        }
        let row = RadioRow { id: b.id, name, visibility: b.visibility.unwrap_or(Visibility::Public), listen_key: users::listen_key(), created_at: store::now() };
        s.store.insert_radio(&row)?;
        s.store.set_on_air(&row.id, b.playlist_id.as_deref())?;
        let r = Radio::new(row);
        radios.push(r.clone());
        r
    };
    radio::start(&s, &r);
    tracing::info!(actor = %auth.name(), radio = %r.id, "radio created");
    radios_event(&s);
    Ok((StatusCode::CREATED, Json(radio_json(&s, &r))))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PatchRadio {
    name: Option<String>,
    visibility: Option<Visibility>,
}

async fn patch_radio(State(s): State<AppState>, auth: Auth, Path(id): Path<String>, Body(b): Body<PatchRadio>) -> Result<Json<Value>, ApiError> {
    auth.require(Scope::Admin)?;
    limit(&s, &auth, LimitKind::Write)?;
    let r = s.radio(&id).ok_or_else(|| ApiError::not_found("no such radio"))?;
    if let Some(n) = b.name.as_deref() {
        let n = radio_name(n)?;
        s.store.update_radio(&id, Some(&n), None)?;
        r.row.lock().unwrap().name = n;
    }
    if let Some(v) = b.visibility {
        s.store.update_radio(&id, None, Some(v))?;
        let old = {
            let mut row = r.row.lock().unwrap();
            let old = crate::relay::path(&row);
            (std::mem::replace(&mut row.visibility, v) != v).then_some(old)
        };
        if let Some(old) = old {
            r.access.send_modify(|n| *n += 1); // listeners on the old address are dropped
            if s.cfg.relay.enabled {
                tokio::spawn(crate::relay::kick(s.clone(), old));
            }
        }
    }
    radios_event(&s);
    Ok(Json(radio_json(&s, &r)))
}

async fn delete_radio(State(s): State<AppState>, auth: Auth, Path(id): Path<String>) -> Result<StatusCode, ApiError> {
    auth.require(Scope::Admin)?;
    limit(&s, &auth, LimitKind::Write)?;
    if id == "main" {
        return Err(ApiError::conflict("cannot_remove_main", "the main radio cannot be removed"));
    }
    let r = {
        let mut radios = s.radios.lock().unwrap();
        let idx = radios.iter().position(|r| r.id == id).ok_or_else(|| ApiError::not_found("no such radio"))?;
        radios.remove(idx)
    };
    r.closed.send_replace(true); // ends playout, encoders and open streams
    let jobs = {
        let mut q = r.queue.lock().unwrap();
        let jobs = q.replace(|_| true);
        q.entries.clear();
        jobs
    };
    jobs::cancel(&s, &jobs);
    s.store.delete_radio(&id)?;
    tracing::info!(actor = %auth.name(), radio = %id, "radio removed");
    radios_event(&s);
    Ok(StatusCode::NO_CONTENT)
}

/// One open pull stream; counted in the radio's listeners until it ends.
struct Listening(Arc<Radio>);

impl Drop for Listening {
    fn drop(&mut self) {
        self.0.listeners.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }
}

/// An Icecast-style pull stream (SPEC 7). It ends when the radio is deleted or its visibility or key changes.
/// A slow listener misses bytes; MP3 and ADTS frames resync.
fn pull(r: Arc<Radio>, bus: &tokio::sync::broadcast::Sender<bytes::Bytes>, content_type: &'static str, icy: bool) -> Response {
    r.listeners.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut access = r.access.subscribe();
    let mut closed = r.closed.subscribe();
    let stop = Box::pin(async move {
        tokio::select! {
            _ = access.changed() => {}
            _ = closed.changed() => {}
        }
    });
    let radio = r.clone();
    let guard = Listening(r);
    let stream = BroadcastStream::new(bus.subscribe()).filter_map(|m| m.ok());
    let stream = futures::StreamExt::take_until(stream, stop);
    let mut inter = icy.then(crate::icy::Interleaver::new);
    let stream = stream.map(move |chunk: bytes::Bytes| {
        let _counted = &guard;
        let out = match inter.as_mut() {
            Some(i) => bytes::Bytes::from(i.push(&chunk, || stream_title(&radio))),
            None => chunk,
        };
        Ok::<_, Infallible>(out)
    });
    let mut res = ([(header::CONTENT_TYPE, content_type), (header::CACHE_CONTROL, "no-cache")], axum::body::Body::from_stream(stream)).into_response();
    if icy {
        res.headers_mut().insert("icy-metaint", header::HeaderValue::from_static("16000"));
    }
    res
}

/// `Title - Artist` of what plays now on `r`, for ICY listeners; empty while silent.
fn stream_title(r: &Radio) -> String {
    r.playout.lock().unwrap().now.as_ref().map(|n| format!("{} - {}", n.track.title, n.track.artist)).unwrap_or_default()
}

/// Players ask for in-stream titles with `Icy-MetaData: 1` (SPEC 10, Tags in the streams).
fn wants_icy(h: &HeaderMap) -> bool {
    h.get("icy-metadata").and_then(|v| v.to_str().ok()).is_some_and(|v| v.trim() == "1")
}

/// A public radio by id, or `None` when it is unknown or private.
fn public_radio(s: &AppState, id: &str) -> Option<Arc<Radio>> {
    s.radio(id).filter(|r| r.visibility() == Visibility::Public)
}

/// A private radio by its listen key.
fn private_radio(s: &AppState, key: &str) -> Option<Arc<Radio>> {
    s.radios().into_iter().find(|r| {
        let row = r.row.lock().unwrap();
        row.visibility == Visibility::Private && row.listen_key == key
    })
}

#[derive(Clone, Copy)]
enum Format {
    Mp3,
    Aac,
}

fn stream_of(r: Option<Arc<Radio>>, f: Format, icy: bool) -> Response {
    match (r, f) {
        (Some(r), Format::Mp3) => {
            let bus = r.mp3.clone();
            pull(r, &bus, "audio/mpeg", icy)
        }
        (Some(r), Format::Aac) => {
            let bus = r.aac.clone();
            pull(r, &bus, "audio/aac", icy)
        }
        (None, _) => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Serves one HLS file of a radio. Only plain names: no slashes, no `..`, no leading dot.
async fn hls_file(s: &AppState, r: Option<Arc<Radio>>, file: &str, req: axum::extract::Request) -> Response {
    let plain = !file.is_empty() && file.len() <= 64 && !file.starts_with('.')
        && file.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b));
    let Some(r) = r.filter(|_| plain) else { return StatusCode::NOT_FOUND.into_response() };
    match ServeFile::new(s.hls_dir(&r.id).join(file)).oneshot(req).await {
        Ok(res) => res.into_response(),
        Err(never) => match never {},
    }
}

/// A radio's listen address, for the formats this server serves (SPEC 7).
fn listen_json(s: &AppState, r: &Radio) -> Value {
    let row = r.row.lock().unwrap();
    let base = match row.visibility {
        Visibility::Public => format!("/radios/{}", row.id),
        Visibility::Private => format!("/listen/{}", row.listen_key),
    };
    let mut v = serde_json::Map::new();
    if s.cfg.serve.mp3 {
        v.insert("mp3".into(), json!(format!("{base}/stream.mp3")));
    }
    if s.cfg.serve.aac {
        v.insert("aac".into(), json!(format!("{base}/stream.aac")));
    }
    if s.cfg.serve.hls {
        v.insert("hls".into(), json!(format!("{base}/hls/index.m3u8")));
    }
    if s.cfg.relay.enabled {
        v.extend(crate::relay::addresses(&s.cfg.relay, &crate::relay::path(&row)));
    }
    Value::Object(v)
}

/// A new secret listen address; the old one stops working at once and its listeners are dropped (SPEC 10).
async fn new_listen_key(State(s): State<AppState>, auth: Auth, Path(id): Path<String>) -> Result<Json<Value>, ApiError> {
    auth.require(Scope::Admin)?;
    limit(&s, &auth, LimitKind::Write)?;
    let r = s.radio(&id).ok_or_else(|| ApiError::not_found("no such radio"))?;
    let key = users::listen_key();
    s.store.set_listen_key(&id, &key)?;
    // Only a private radio's address carries the key; a public radio keeps its listeners.
    let moved = {
        let mut row = r.row.lock().unwrap();
        let old = crate::relay::path(&row);
        row.listen_key = key;
        (row.visibility == Visibility::Private).then_some(old)
    };
    if let Some(old) = moved {
        r.access.send_modify(|n| *n += 1);
        if s.cfg.relay.enabled {
            tokio::spawn(crate::relay::kick(s.clone(), old));
        }
    }
    tracing::info!(actor = %auth.name(), radio = %id, "new listen key");
    radios_event(&s);
    Ok(Json(radio_json(&s, &r)))
}

/// A file of the web UI, built into the binary; `no-cache` so a new build shows at once.
fn asset(kind: &'static str, body: &'static str) -> Response {
    ([(header::CONTENT_TYPE, kind), (header::CACHE_CONTROL, "no-cache")], body).into_response()
}
