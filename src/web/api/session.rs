//! Signing in and out, and changing the password (SPEC 4, 10).

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;

use super::{Body, blocking};
use crate::web::{
    auth::{ClientAddr, Me, SESSION_COOKIE, same_origin},
    error::ApiError,
    state::Shared,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignIn {
    username: String,
    password: String,
    #[serde(default)]
    remember: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangePassword {
    current: String,
    new: String,
}

fn session_cookie(app: &Shared, value: &str, max_age_s: u64) -> String {
    let secure = if app.secure_cookies { "; Secure" } else { "" };
    format!("{SESSION_COOKIE}={value}; HttpOnly; SameSite=Strict; Path=/; Max-Age={max_age_s}{secure}")
}

pub async fn sign_in(State(app): State<Shared>, ClientAddr(ip): ClientAddr, headers: HeaderMap, Body(b): Body<SignIn>) -> Result<Response, ApiError> {
    if !same_origin(&headers, app.public_origin.as_deref()) {
        return Err(ApiError::forbidden("bad_origin", "the request did not come from this site"));
    }
    let client = ip.map(|i| i.to_string());
    let signed = blocking(&app, move |a| a.account.sign_in(&b.username, &b.password, b.remember, client.as_deref())).await.inspect_err(|e| {
        if e.code == "bad_credentials" {
            tracing::warn!(client = ?ip, "a sign-in failed");
        }
    })?;
    Ok((StatusCode::NO_CONTENT, [(header::SET_COOKIE, session_cookie(&app, &signed.session, signed.max_age_s))]).into_response())
}

pub async fn sign_out(State(app): State<Shared>, me: Me) -> Result<Response, ApiError> {
    if let Some(session) = me.session {
        blocking(&app, move |a| a.account.sign_out(&session)).await?;
    }
    Ok((StatusCode::NO_CONTENT, [(header::SET_COOKIE, session_cookie(&app, "", 0))]).into_response())
}

pub async fn change_password(State(app): State<Shared>, me: Me, Body(b): Body<ChangePassword>) -> Result<StatusCode, ApiError> {
    blocking(&app, move |a| a.account.change_password(me.session.as_deref(), &b.current, &b.new)).await?;
    tracing::info!("the password was changed");
    Ok(StatusCode::NO_CONTENT)
}
