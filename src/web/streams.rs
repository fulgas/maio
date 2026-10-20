//! Live streams and the relay's auth hook (arc42 8.4): no session; a path answers only while it names a live playlist.

use axum::{
    Json, Router,
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, Request, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use futures::StreamExt;
use tokio_stream::wrappers::{BroadcastStream, errors::BroadcastStreamRecvError};
use tower::ServiceExt;
use tower_http::services::ServeFile;

use serde::Deserialize;

use crate::{
    domain::live::{PathKind, StreamFormat},
    web::{icy, state::Shared},
};

pub fn router() -> Router<Shared> {
    Router::new()
        .route("/live/{name}/{file}", get(live_stream))
        .route("/listen/{name}/{file}", get(listen_stream))
        .route("/live/{name}/hls/{file}", get(live_hls))
        .route("/listen/{name}/hls/{file}", get(listen_hls))
        .route("/relay/auth", post(auth))
}

async fn live_stream(state: State<Shared>, path: Path<(String, String)>, headers: HeaderMap) -> Response {
    stream(state, PathKind::Live, path, headers).await
}

async fn listen_stream(state: State<Shared>, path: Path<(String, String)>, headers: HeaderMap) -> Response {
    stream(state, PathKind::Listen, path, headers).await
}

async fn live_hls(state: State<Shared>, path: Path<(String, String)>) -> Response {
    hls(state, PathKind::Live, path).await
}

async fn listen_hls(state: State<Shared>, path: Path<(String, String)>) -> Response {
    hls(state, PathKind::Listen, path).await
}

fn not_found() -> Response {
    StatusCode::NOT_FOUND.into_response()
}

async fn stream(State(app): State<Shared>, kind: PathKind, Path((name, file)): Path<(String, String)>, headers: HeaderMap) -> Response {
    let format = match file.as_str() {
        "stream.mp3" => StreamFormat::Mp3,
        "stream.aac" => StreamFormat::Aac,
        _ => return not_found(),
    };
    let Some((id, listen)) = app.live.listen(kind, &name, format) else { return not_found() };
    let icy = headers.get("icy-metadata").is_some_and(|v| v == "1");
    let (mut moved, guard) = (listen.moved, listen.guard);
    let mut stopping = app.stopping.subscribe();
    let live = app.live.clone();
    let mut interleave = icy::Interleaver::new();
    let bytes = BroadcastStream::new(listen.bytes)
        // A listener too far behind is dropped: it reconnects; everyone else keeps going.
        .take_while(|r| std::future::ready(!matches!(r, Err(BroadcastStreamRecvError::Lagged(_)))))
        .filter_map(|r| std::future::ready(r.ok()))
        .map(move |chunk| {
            let _counted = &guard; // the listener counts until this stream is dropped
            let out = if icy {
                let (id, live) = (id.clone(), live.clone());
                interleave.push(&chunk, move || format!("{} - Maio", live.now_title(&id).unwrap_or_default()))
            } else {
                chunk.to_vec()
            };
            Ok::<_, std::io::Error>(bytes::Bytes::from(out))
        })
        // A new address, off air, or the server stopping ends the stream.
        .take_until(async move {
            tokio::select! {
                _ = moved.changed() => {}
                _ = stopping.wait_for(|s| *s) => {}
            }
        });
    let mut res = Body::from_stream(bytes).into_response();
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, header::HeaderValue::from_static(if format == StreamFormat::Mp3 { "audio/mpeg" } else { "audio/aac" }));
    h.insert(header::CACHE_CONTROL, header::HeaderValue::from_static("no-store"));
    if icy {
        h.insert("icy-metaint", header::HeaderValue::from(icy::METAINT));
    }
    res
}

async fn hls(State(app): State<Shared>, kind: PathKind, Path((name, file)): Path<(String, String)>) -> Response {
    let safe = !file.starts_with('.') && !file.is_empty() && file.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-');
    if !safe {
        return not_found();
    }
    let Some(dir) = app.live.hls_dir(kind, &name) else { return not_found() };
    match ServeFile::new(dir.join(file)).oneshot(Request::new(Body::empty())).await {
        Ok(r) if r.status().is_success() => r.into_response(),
        _ => not_found(),
    }
}

/// What MediaMTX sends to its HTTP auth hook.
#[derive(Debug, Deserialize)]
struct AuthRequest {
    #[serde(default)]
    user: String,
    #[serde(default)]
    password: String,
    #[serde(default)]
    action: String,
    #[serde(default)]
    path: String,
}

async fn auth(State(app): State<Shared>, Json(r): Json<AuthRequest>) -> StatusCode {
    if app.live.authorize(&r.user, &r.password, &r.action, &r.path) { StatusCode::OK } else { StatusCode::UNAUTHORIZED }
}
