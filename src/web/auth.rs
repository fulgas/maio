//! Who is calling (SPEC 4): you, by session cookie or by the API token.

use std::{
    convert::Infallible,
    net::{IpAddr, SocketAddr},
};

use axum::{
    extract::{ConnectInfo, FromRequestParts},
    http::{HeaderMap, Method, header, request::Parts},
};

use crate::web::{api::blocking, error::ApiError, state::Shared};

pub const SESSION_COOKIE: &str = "maio_session";

/// You, signed in. `session` is `None` when the call came with the API token.
pub struct Me {
    pub session: Option<String>,
}

pub fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|kv| kv.trim().strip_prefix(name).and_then(|rest| rest.strip_prefix('=')).map(str::to_string))
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers.get(header::AUTHORIZATION)?.to_str().ok()?.strip_prefix("Bearer ")
}

/// True when `Origin` is this site (SPEC 4, cross-site protection): the configured public origin when a proxy
/// sits in front, else the host the request was sent to.
pub fn same_origin(headers: &HeaderMap, public_origin: Option<&str>) -> bool {
    if let Some(public) = public_origin {
        return headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()).is_some_and(|o| o.trim_end_matches('/').eq_ignore_ascii_case(public));
    }
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok());
    let origin = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()).and_then(|o| o.split_once("://").map(|(_, h)| h));
    matches!((host, origin), (Some(h), Some(o)) if h.eq_ignore_ascii_case(o.trim_end_matches('/')))
}

impl FromRequestParts<Shared> for Me {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, app: &Shared) -> Result<Self, ApiError> {
        if let Some(token) = bearer(&parts.headers) {
            return if app.account.is_api_token(token) { Ok(Me { session: None }) } else { Err(ApiError::unauthorized("wrong API token")) };
        }
        let session = cookie(&parts.headers, SESSION_COOKIE).ok_or_else(|| ApiError::unauthorized("sign in first"))?;
        let check = session.clone();
        if !blocking(app, move |a| a.account.is_signed_in(&check)).await? {
            return Err(ApiError::unauthorized("the session ended; sign in again"));
        }
        if !matches!(parts.method, Method::GET | Method::HEAD) && !same_origin(&parts.headers, app.public_origin.as_deref()) {
            return Err(ApiError::forbidden("bad_origin", "the request did not come from this site"));
        }
        Ok(Me { session: Some(session) })
    }
}

/// The client's address, when the server runs with connect info (tests run without).
pub struct ClientAddr(pub Option<IpAddr>);

impl<S: Send + Sync> FromRequestParts<S> for ClientAddr {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Infallible> {
        Ok(Self(parts.extensions.get::<ConnectInfo<SocketAddr>>().map(|c| c.0.ip())))
    }
}
