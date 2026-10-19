//! Errors as the API sends them (SPEC 10): `{"error": {"code", "message", ...}}` with a matching status.

use axum::{
    Json,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use crate::domain::Error as Rule;
use serde_json::{Map, Value, json};

#[derive(Debug, Clone, PartialEq)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    pub retry_after: Option<u64>,
    /// More fields for the error object, such as the playlists that hold a song.
    pub extra: Map<String, Value>,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self { status, code, message: message.into(), retry_after: None, extra: Map::new() }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthorized", message)
    }

    pub fn forbidden(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, code, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", message)
    }

    pub fn invalid(code: &'static str, message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, code, message)
    }

    /// The details go to the log, never to the caller.
    pub fn internal() -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", "something went wrong on the server; the log has the details")
    }
}

impl From<Rule> for ApiError {
    fn from(rule: Rule) -> Self {
        let status = match &rule {
            Rule::NotFound(_) => StatusCode::NOT_FOUND,
            Rule::Invalid { code: "job_finished", .. } => StatusCode::CONFLICT,
            Rule::Invalid { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            Rule::SongInPlaylist(_) | Rule::PlaylistLive | Rule::NoncommercialEngine => StatusCode::CONFLICT,
            Rule::EngineUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            Rule::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
            Rule::BadCredentials => StatusCode::UNAUTHORIZED,
        };
        let mut e = Self::new(status, rule.code(), rule.to_string());
        match rule {
            Rule::SongInPlaylist(playlists) => {
                e.extra.insert("playlists".into(), json!(playlists));
            }
            Rule::RateLimited { retry_after_s } => e.retry_after = Some(retry_after_s),
            _ => {}
        }
        e
    }
}

impl From<crate::usecases::Error> for ApiError {
    fn from(e: crate::usecases::Error) -> Self {
        match e {
            crate::usecases::Error::Rule(rule) => rule.into(),
            crate::usecases::Error::Port(e) => {
                tracing::error!("a port failed: {e:#}");
                Self::internal()
            }
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut error = Map::new();
        error.insert("code".into(), json!(self.code));
        error.insert("message".into(), json!(self.message));
        error.extend(self.extra);
        let mut res = (self.status, Json(json!({ "error": error }))).into_response();
        if let Some(secs) = self.retry_after {
            res.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from(secs));
        }
        res
    }
}
