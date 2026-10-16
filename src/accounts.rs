//! Sign-in, sessions and users (SPEC 8).

use std::{
    collections::{HashMap, VecDeque},
    convert::Infallible,
    net::{IpAddr, SocketAddr},
    sync::Mutex,
    time::{Duration, Instant},
};

use axum::{
    Json,
    extract::{ConnectInfo, FromRequestParts, Path, State},
    http::{HeaderMap, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    api::{Body, SESSION_COOKIE, limit, same_origin},
    auth::{Auth, LimitKind},
    config::Scope,
    error::ApiError,
    state::AppState,
    store::{self, User},
    users::{self, Role},
};

const WINDOW: Duration = Duration::from_secs(15 * 60);
const SESSION_S: i64 = 12 * 3600;
const REMEMBER_S: i64 = 30 * 24 * 3600;

/// Failed sign-ins per username and per client address, in memory (SPEC 8, Sign-in limits).
#[derive(Default)]
pub struct SignInLimiter {
    fails: Mutex<HashMap<String, VecDeque<Instant>>>,
}

impl SignInLimiter {
    /// Reads only: a key enters the map on a real failure, and leaves when its window empties.
    fn blocked(&self, key: &str, max: usize, now: Instant) -> Option<u64> {
        let mut fails = self.fails.lock().unwrap();
        let q = fails.get_mut(key)?;
        while q.front().is_some_and(|&f| now.duration_since(f) >= WINDOW) {
            q.pop_front();
        }
        if q.is_empty() {
            fails.remove(key);
            return None;
        }
        (q.len() >= max).then(|| (WINDOW - now.duration_since(*q.front().expect("not empty"))).as_secs().max(1))
    }

    fn fail(&self, key: &str, now: Instant) {
        self.fails.lock().unwrap().entry(key.to_string()).or_default().push_back(now);
    }

    /// How many keys are tracked; for tests and diagnostics.
    pub fn keys(&self) -> usize {
        self.fails.lock().unwrap().len()
    }

    fn clear(&self, key: &str) {
        self.fails.lock().unwrap().remove(key);
    }
}

/// The client's address when the server runs with connect info (absent in tests).
pub struct ClientAddr(pub Option<IpAddr>);

impl<S: Send + Sync> FromRequestParts<S> for ClientAddr {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Infallible> {
        Ok(Self(parts.extensions.get::<ConnectInfo<SocketAddr>>().map(|c| c.0.ip())))
    }
}

fn bad_credentials() -> ApiError {
    ApiError::new(StatusCode::UNAUTHORIZED, "bad_credentials", "wrong username or password")
}

fn session_cookie(s: &AppState, id: &str, max_age: i64) -> String {
    let secure = if s.cfg.secure_cookies { "; Secure" } else { "" };
    format!("{SESSION_COOKIE}={id}; HttpOnly; SameSite=Strict; Path=/; Max-Age={max_age}{secure}")
}

/// Argon2 is deliberately slow; keep it off the async workers.
async fn verify(password: String, hash: Option<String>) -> bool {
    tokio::task::spawn_blocking(move || match hash {
        Some(h) => users::verify_password(&password, &h),
        None => {
            users::verify_dummy(&password);
            false
        }
    })
    .await
    .unwrap_or(false)
}

async fn hash(password: String) -> Result<String, ApiError> {
    tokio::task::spawn_blocking(move || users::hash_password(&password)).await.map_err(|e| ApiError::internal(e.to_string()))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignIn {
    username: String,
    password: String,
    #[serde(default)]
    remember: bool,
}

pub async fn sign_in(State(s): State<AppState>, headers: HeaderMap, ClientAddr(ip): ClientAddr, Body(b): Body<SignIn>) -> Result<Response, ApiError> {
    if !same_origin(&headers) {
        return Err(ApiError::forbidden("bad_origin", "request did not come from this site"));
    }
    if !users::valid_username(&b.username) {
        verify(b.password, None).await; // same cost as a real check, and no limiter state for junk names
        return Err(bad_credentials());
    }
    let now = Instant::now();
    let user_key = format!("u:{}", b.username);
    let ip_key = ip.map(|i| format!("ip:{i}"));
    let blocked = s.signin.blocked(&user_key, 5, now).or_else(|| ip_key.as_deref().and_then(|k| s.signin.blocked(k, 20, now)));
    if let Some(wait) = blocked {
        return Err(ApiError::rate_limited(Some(wait)));
    }
    let user = s.store.user(&b.username)?;
    if !verify(b.password, user.as_ref().map(|u| u.hash.clone())).await {
        s.signin.fail(&user_key, now);
        if let Some(k) = &ip_key {
            s.signin.fail(k, now);
        }
        tracing::warn!(username = %b.username, client = ?ip, "sign-in failed");
        return Err(bad_credentials());
    }
    let user = user.expect("verified users exist");
    s.signin.clear(&user_key);
    let ts = chrono::Utc::now().timestamp();
    s.store.delete_expired_sessions(ts)?;
    let id = users::new_secret();
    let ttl = if b.remember { REMEMBER_S } else { SESSION_S };
    s.store.create_session(&id, &user.username, ts + ttl, b.remember)?;
    s.store.touch_user(&user.username)?;
    tracing::info!(user = %user.username, "signed in");
    Ok((StatusCode::NO_CONTENT, [(header::SET_COOKIE, session_cookie(&s, &id, ttl))]).into_response())
}

pub async fn sign_out(State(s): State<AppState>, auth: Auth) -> Result<Response, ApiError> {
    s.store.delete_session(&auth.session)?;
    Ok((StatusCode::NO_CONTENT, [(header::SET_COOKIE, session_cookie(&s, "", 0))]).into_response())
}

pub async fn me(auth: Auth) -> Result<Json<Value>, ApiError> {
    let u = &auth.user;
    Ok(Json(json!({"username": u.username, "name": u.name, "role": u.role, "must_change_password": u.must_change})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangePassword {
    current: String,
    new: String,
}

pub async fn change_password(State(s): State<AppState>, auth: Auth, Body(b): Body<ChangePassword>) -> Result<StatusCode, ApiError> {
    let u = auth.user.clone();
    if !verify(b.current, Some(u.hash.clone())).await {
        return Err(ApiError::forbidden("bad_credentials", "the current password is wrong"));
    }
    if !users::valid_password(&b.new) {
        return Err(ApiError::invalid("weak_password", "passwords are 10 to 128 characters"));
    }
    s.store.set_password(&u.username, &hash(b.new).await?, false)?;
    s.store.delete_sessions_for(&u.username, Some(&auth.session))?;
    tracing::info!(user = %u.username, "changed password");
    Ok(StatusCode::NO_CONTENT)
}

/// Users are managed by signed-in admins (SPEC 8).
fn admin(s: &AppState, auth: &Auth, writes: bool) -> Result<(), ApiError> {
    auth.require(Scope::Admin)?;
    if writes {
        limit(s, auth, LimitKind::Write)?;
    }
    Ok(())
}

fn valid_name(name: &str) -> Result<String, ApiError> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > 80 {
        return Err(ApiError::invalid("bad_name", "name must be 1 to 80 characters"));
    }
    Ok(n.to_string())
}

fn role(r: &str) -> Result<Role, ApiError> {
    Role::parse(r).ok_or_else(|| ApiError::invalid("bad_role", "role is admin, operator or listener"))
}

pub async fn list_users(State(s): State<AppState>, auth: Auth) -> Result<Json<Value>, ApiError> {
    admin(&s, &auth, false)?;
    Ok(Json(json!(s.store.users()?)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewUser {
    name: String,
    username: String,
    password: String,
    role: String,
}

pub async fn create_user(State(s): State<AppState>, auth: Auth, Body(b): Body<NewUser>) -> Result<(StatusCode, Json<Value>), ApiError> {
    admin(&s, &auth, true)?;
    if !users::valid_username(&b.username) {
        return Err(ApiError::invalid("bad_username", "usernames are 2 to 32 characters from a-z 0-9 . _ -"));
    }
    if !users::valid_password(&b.password) {
        return Err(ApiError::invalid("weak_password", "passwords are 10 to 128 characters"));
    }
    let role = role(&b.role)?;
    let name = valid_name(&b.name)?;
    if s.store.user(&b.username)?.is_some() {
        return Err(ApiError::conflict("user_exists", "that username is taken"));
    }
    let user = User {
        username: b.username,
        name,
        role: role.as_str().into(),
        hash: hash(b.password).await?,
        must_change: true,
        created_at: store::now(),
        last_seen_at: None,
    };
    s.store.insert_user(&user)?;
    tracing::info!(actor = %auth.name(), user = %user.username, role = %user.role, "user added");
    Ok((StatusCode::CREATED, Json(json!(user))))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchUser {
    name: Option<String>,
    role: Option<String>,
}

pub async fn patch_user(State(s): State<AppState>, auth: Auth, Path(username): Path<String>, Body(b): Body<PatchUser>) -> Result<Json<Value>, ApiError> {
    admin(&s, &auth, true)?;
    let target = s.store.user(&username)?.ok_or_else(|| ApiError::not_found("no such user"))?;
    let role = b.role.as_deref().map(role).transpose()?;
    let name = b.name.as_deref().map(valid_name).transpose()?;
    if target.role == "admin" && role.is_some_and(|r| r != Role::Admin) && s.store.count_admins()? <= 1 {
        return Err(ApiError::conflict("last_admin", "make someone else an admin first"));
    }
    s.store.update_user(&username, name.as_deref(), role.map(Role::as_str))?;
    tracing::info!(actor = %auth.name(), user = %username, "user changed");
    Ok(Json(json!(s.store.user(&username)?)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResetPassword {
    password: String,
}

pub async fn reset_password(State(s): State<AppState>, auth: Auth, Path(username): Path<String>, Body(b): Body<ResetPassword>) -> Result<StatusCode, ApiError> {
    admin(&s, &auth, true)?;
    if !users::valid_password(&b.password) {
        return Err(ApiError::invalid("weak_password", "passwords are 10 to 128 characters"));
    }
    if !s.store.set_password(&username, &hash(b.password).await?, true)? {
        return Err(ApiError::not_found("no such user"));
    }
    s.store.delete_sessions_for(&username, None)?;
    tracing::info!(actor = %auth.name(), user = %username, "password reset");
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_user(State(s): State<AppState>, auth: Auth, Path(username): Path<String>) -> Result<StatusCode, ApiError> {
    admin(&s, &auth, true)?;
    if auth.user.username == username {
        return Err(ApiError::conflict("cannot_remove_self", "ask another admin to remove you"));
    }
    let target = s.store.user(&username)?.ok_or_else(|| ApiError::not_found("no such user"))?;
    if target.role == "admin" && s.store.count_admins()? <= 1 {
        return Err(ApiError::conflict("last_admin", "make someone else an admin first"));
    }
    s.store.delete_user(&username)?;
    tracing::info!(actor = %auth.name(), user = %username, "user removed");
    Ok(StatusCode::NO_CONTENT)
}
