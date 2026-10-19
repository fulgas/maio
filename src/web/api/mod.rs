//! The JSON API under `/api/v1` (SPEC 10): everything the web app does, a script can do.

mod events;
mod playlists;
mod session;
mod songs;
mod status;

use axum::{
    Json, Router,
    extract::{FromRequest, FromRequestParts, Query, Request},
    http::request::Parts,
    routing::{delete, get, post},
};
use serde::de::DeserializeOwned;

use crate::web::{
    error::ApiError,
    state::{App, Shared},
};

pub fn router(app: Shared) -> Router {
    let api = Router::new()
        .route("/session", post(session::sign_in).delete(session::sign_out))
        .route("/me/password", post(session::change_password))
        .route("/status", get(status::status))
        .route("/events", get(events::events))
        .route("/songs", get(songs::list).post(songs::make))
        .route("/songs/{id}", get(songs::get).patch(songs::rename).delete(songs::delete))
        .route("/songs/{id}/audio", get(songs::audio))
        .route("/jobs", get(songs::jobs))
        .route("/jobs/{id}", delete(songs::cancel))
        .route("/playlists", get(playlists::list).post(playlists::create))
        .route("/playlists/{id}", get(playlists::get).patch(playlists::rename).delete(playlists::delete))
        .route("/playlists/{id}/songs", post(playlists::add).put(playlists::reorder))
        .route("/playlists/{id}/songs/{song_id}", delete(playlists::remove))
        .route_layer(axum::middleware::from_fn_with_state(app.metrics.clone(), crate::web::metrics::record))
        .fallback(|| async { ApiError::not_found("no such API path") });
    Router::new()
        .nest("/api/v1", api)
        .merge(crate::web::pages::router(app.clone()))
        .layer(
            // A span per request at INFO, so the default filter keeps it (tower-http makes them at DEBUG).
            tower_http::trace::TraceLayer::new_for_http()
                .make_span_with(tower_http::trace::DefaultMakeSpan::new().level(tracing::Level::INFO))
                .on_response(tower_http::trace::DefaultOnResponse::new().level(tracing::Level::INFO)),
        )
        .with_state(app)
}

/// A JSON body whose rejections use the API's error format.
pub struct Body<T>(pub T);

impl<T: DeserializeOwned, S: Send + Sync> FromRequest<S> for Body<T> {
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, ApiError> {
        Json::<T>::from_request(req, state).await.map(|Json(v)| Body(v)).map_err(|e| ApiError::invalid("invalid_body", e.body_text()))
    }
}

/// Query parameters whose rejections use the API's error format.
pub struct Params<T>(pub T);

impl<T: DeserializeOwned, S: Send + Sync> FromRequestParts<S> for Params<T> {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, ApiError> {
        Query::<T>::from_request_parts(parts, state).await.map(|Query(v)| Params(v)).map_err(|e| ApiError::invalid("invalid_query", e.body_text()))
    }
}

/// Runs a use case on a blocking thread: they touch SQLite, ffmpeg or Argon2.
pub async fn blocking<T: Send + 'static>(app: &Shared, f: impl FnOnce(&App) -> crate::usecases::Result<T> + Send + 'static) -> Result<T, ApiError> {
    let app = app.clone();
    tokio::task::spawn_blocking(move || f(&app))
        .await
        .map_err(|e| {
            tracing::error!("a blocking task failed: {e}");
            ApiError::internal()
        })?
        .map_err(ApiError::from)
}
