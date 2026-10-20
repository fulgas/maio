//! The JSON API (SPEC 10) as the browser calls it: the session cookie goes along by itself, and fetch sends `Origin`.
//! The network functions run only in the browser; the types and helpers are plain Rust.

use gloo_net::http::{Method, RequestBuilder};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::json;

use crate::domain::{
    Live, Playlist, PlaylistRef, Song,
    engine::{EngineInfo, Health},
    job::Job,
    relay::RelayStatus,
    request::SongRequest,
};

const BASE: &str = "/api/v1";

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SongView {
    #[serde(flatten)]
    pub song: Song,
    pub playlists: Vec<PlaylistRef>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SongPage {
    pub songs: Vec<SongView>,
    pub next: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PlaylistSummary {
    pub id: String,
    pub name: String,
    pub songs: usize,
    pub seconds: f64,
    pub live: Live,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PlaylistView {
    pub playlist: Playlist,
    pub songs: Vec<Song>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct EngineStatus {
    pub id: String,
    pub info: Option<EngineInfo>,
    pub health: Health,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TextEngine {
    pub id: String,
    pub healthy: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Status {
    pub engines: Vec<EngineStatus>,
    pub text_engines: Vec<TextEngine>,
    /// MediaMTX, when it relays the live playlists.
    #[serde(default)]
    pub relay: Option<RelayStatus>,
    pub moods: Vec<String>,
    pub genres: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Accepted {
    pub job_id: String,
    pub eta_s: Option<u32>,
}

/// A call that did not work, as the person sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct Failure {
    pub status: u16,
    pub code: String,
    pub message: String,
    pub playlists: Vec<PlaylistRef>,
    pub retry_after: Option<u64>,
}

#[derive(Deserialize)]
struct ErrorBody {
    error: ErrorFields,
}

#[derive(Deserialize)]
struct ErrorFields {
    code: String,
    message: String,
    #[serde(default)]
    playlists: Vec<PlaylistRef>,
}

fn sentence(s: &str) -> String {
    let mut c = s.chars();
    let mut out: String = c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default();
    if !out.ends_with('.') {
        out.push('.');
    }
    out
}

impl Failure {
    pub fn parse(status: u16, body: &str, retry_after: Option<u64>) -> Self {
        match serde_json::from_str::<ErrorBody>(body) {
            Ok(b) => Self { status, code: b.error.code, message: b.error.message, playlists: b.error.playlists, retry_after },
            Err(_) => Self { status, code: "unexpected".into(), message: format!("Maio answered {status}"), playlists: vec![], retry_after },
        }
    }

    fn network(e: impl std::fmt::Display) -> Self {
        Self { status: 0, code: "network".into(), message: format!("could not reach Maio ({e})"), playlists: vec![], retry_after: None }
    }

    /// What a toast or a form error says.
    pub fn text(&self) -> String {
        match self.code.as_str() {
            "song_in_playlist" => {
                let names: Vec<&str> = self.playlists.iter().map(|p| p.name.as_str()).collect();
                let list = match names.as_slice() {
                    [rest @ .., last] if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
                    _ => names.join(""),
                };
                format!("Remove it from {list} first.")
            }
            "live_limit" => {
                let names: Vec<&str> = self.playlists.iter().map(|p| p.name.as_str()).collect();
                let list = match names.as_slice() {
                    [rest @ .., last] if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
                    _ => names.join(""),
                };
                format!("{} {list} {} live now.", sentence(&self.message), if names.len() == 1 { "is" } else { "are" })
            }
            "rate_limited" => {
                let wait = self.retry_after.map_or_else(
                    || "a moment".to_string(),
                    |s| crate::ui::format::eta(Some(u32::try_from(s).unwrap_or(u32::MAX))).replace("about ", ""),
                );
                format!("{} Try again in {wait}.", sentence(&self.message))
            }
            "unexpected" => format!("{}. Try again.", self.message),
            _ => sentence(&self.message),
        }
    }
}

/// Percent-encodes a query value (RFC 3986 unreserved characters pass through).
pub fn enc(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// The query takes the format's name, lower case: mp3, aac, flac, wav, opus.
pub fn download_url(id: &str, format: crate::domain::DownloadFormat) -> String {
    format!("{BASE}/songs/{}/download?format={}", enc(id), format.label().to_ascii_lowercase())
}

pub fn audio_url(id: &str) -> String {
    format!("{BASE}/songs/{}/audio", enc(id))
}

/// The session ended (`401 unauthorized`); a wrong password (`401 bad_credentials`) stays on its form.
pub fn needs_sign_in(status: u16, code: &str) -> bool {
    status == 401 && code == "unauthorized"
}

async fn call<T: DeserializeOwned>(method: Method, path: &str, body: Option<serde_json::Value>) -> Result<T, Failure> {
    let builder = RequestBuilder::new(&format!("{BASE}{path}")).method(method);
    let req = match body {
        Some(b) => builder.json(&b),
        None => builder.build(),
    }
    .map_err(Failure::network)?;
    let res = req.send().await.map_err(Failure::network)?;
    let status = res.status();
    let retry_after = res.headers().get("retry-after").and_then(|v| v.parse().ok());
    let text = res.text().await.map_err(Failure::network)?;
    if (200..300).contains(&status) {
        // `()` reads from `null`, for 204 No Content.
        return serde_json::from_str(if text.is_empty() { "null" } else { &text }).map_err(Failure::network);
    }
    let failure = Failure::parse(status, &text, retry_after);
    if needs_sign_in(status, &failure.code)
        && let Some(w) = web_sys::window()
    {
        let _ = w.location().set_href("/sign-in");
    }
    Err(failure)
}

fn body(v: impl Serialize) -> Option<serde_json::Value> {
    serde_json::to_value(v).ok()
}

pub async fn sign_in(username: &str, password: &str, remember: bool) -> Result<(), Failure> {
    call(Method::POST, "/session", body(json!({"username": username, "password": password, "remember": remember}))).await
}
pub async fn sign_out() -> Result<(), Failure> {
    call(Method::DELETE, "/session", None).await
}
pub async fn change_password(current: &str, new: &str) -> Result<(), Failure> {
    call(Method::POST, "/me/password", body(json!({"current": current, "new": new}))).await
}
pub async fn status() -> Result<Status, Failure> {
    call(Method::GET, "/status", None).await
}
pub async fn jobs() -> Result<Vec<Job>, Failure> {
    call(Method::GET, "/jobs", None).await
}
pub async fn cancel_job(id: &str) -> Result<Job, Failure> {
    call(Method::DELETE, &format!("/jobs/{}", enc(id)), None).await
}
pub async fn make(request: &SongRequest) -> Result<Accepted, Failure> {
    call(Method::POST, "/songs", body(request)).await
}
pub async fn songs(q: &str, cursor: Option<&str>) -> Result<SongPage, Failure> {
    let cursor = cursor.map(|c| format!("&cursor={}", enc(c))).unwrap_or_default();
    call(Method::GET, &format!("/songs?q={}&limit=50{cursor}", enc(q)), None).await
}
pub async fn rename_song(id: &str, title: &str) -> Result<SongView, Failure> {
    call(Method::PATCH, &format!("/songs/{}", enc(id)), body(json!({"title": title}))).await
}
pub async fn delete_song(id: &str) -> Result<(), Failure> {
    call(Method::DELETE, &format!("/songs/{}", enc(id)), None).await
}
pub async fn playlists() -> Result<Vec<PlaylistSummary>, Failure> {
    call(Method::GET, "/playlists", None).await
}
pub async fn create_playlist(name: &str) -> Result<Playlist, Failure> {
    call(Method::POST, "/playlists", body(json!({"name": name}))).await
}
pub async fn playlist(id: &str) -> Result<PlaylistView, Failure> {
    call(Method::GET, &format!("/playlists/{}", enc(id)), None).await
}
pub async fn rename_playlist(id: &str, name: &str) -> Result<Playlist, Failure> {
    call(Method::PATCH, &format!("/playlists/{}", enc(id)), body(json!({"name": name}))).await
}
pub async fn delete_playlist(id: &str) -> Result<(), Failure> {
    call(Method::DELETE, &format!("/playlists/{}", enc(id)), None).await
}
pub async fn add_songs(id: &str, song_ids: &[String]) -> Result<Playlist, Failure> {
    call(Method::POST, &format!("/playlists/{}/songs", enc(id)), body(json!({"song_ids": song_ids}))).await
}
pub async fn reorder(id: &str, song_ids: &[String]) -> Result<Playlist, Failure> {
    call(Method::PUT, &format!("/playlists/{}/songs", enc(id)), body(json!({"song_ids": song_ids}))).await
}
pub async fn remove_song(id: &str, song_id: &str) -> Result<Playlist, Failure> {
    call(Method::DELETE, &format!("/playlists/{}/songs/{}", enc(id), enc(song_id)), None).await
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct OnAirJson {
    pub now: Option<LiveSongJson>,
    pub next: Option<LiveSongJson>,
    pub listeners: usize,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct LiveSongJson {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
pub struct Addresses {
    pub mp3: Option<String>,
    pub aac: Option<String>,
    pub hls: Option<String>,
    pub rtsp: Option<String>,
    pub srt: Option<String>,
    pub rtmp: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct LiveJson {
    pub live: bool,
    pub shuffle: bool,
    pub visibility: crate::domain::Visibility,
    pub on_air: Option<OnAirJson>,
    pub live_count: usize,
    pub max_live: usize,
    pub addresses: Option<Addresses>,
}

pub async fn live(id: &str) -> Result<LiveJson, Failure> {
    call(Method::GET, &format!("/playlists/{}/live", enc(id)), None).await
}
pub async fn set_live(id: &str, change: &serde_json::Value) -> Result<LiveJson, Failure> {
    call(Method::PUT, &format!("/playlists/{}/live", enc(id)), Some(change.clone())).await
}
pub async fn switch_model(engine: &str, model: &str) -> Result<(), Failure> {
    call(Method::PUT, &format!("/engines/{}/model", enc(engine)), body(json!({"model": model}))).await
}
pub async fn new_key(id: &str) -> Result<LiveJson, Failure> {
    call(Method::POST, &format!("/playlists/{}/live/key", enc(id)), None).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_encoding() {
        assert_eq!(enc("rain & jazz #2 100%+"), "rain%20%26%20jazz%20%232%20100%25%2B");
        assert_eq!(enc("café"), "caf%C3%A9");
        assert_eq!(enc("lo-fi_v1.2~"), "lo-fi_v1.2~");
    }

    #[test]
    fn failures_read_the_api_error_body() {
        let body = r#"{"error":{"code":"song_in_playlist","message":"the song is in 2 playlists","playlists":[{"id":"p1","name":"Rainy jazz"},{"id":"p2","name":"Morning run"}]}}"#;
        let f = Failure::parse(409, body, None);
        assert_eq!(f.code, "song_in_playlist");
        assert_eq!(f.playlists.len(), 2);
        assert_eq!(f.text(), "Remove it from Rainy jazz and Morning run first.");
    }

    #[test]
    fn rate_limits_say_when_to_try_again() {
        let f = Failure::parse(429, r#"{"error":{"code":"rate_limited","message":"too many songs asked for"}}"#, Some(90));
        assert_eq!(f.text(), "Too many songs asked for. Try again in 2 min.");
    }

    #[test]
    fn a_body_that_is_not_ours_still_gives_a_message() {
        let f = Failure::parse(502, "<html>bad gateway</html>", None);
        assert_eq!(f.code, "unexpected");
        assert_eq!(f.text(), "Maio answered 502. Try again.");
    }

    #[test]
    fn other_failures_show_the_server_message_as_a_sentence() {
        let f = Failure::parse(404, r#"{"error":{"code":"not_found","message":"no such song"}}"#, None);
        assert_eq!(f.text(), "No such song.");
    }

    #[test]
    fn unauthorised_goes_to_sign_in() {
        assert!(needs_sign_in(401, "unauthorized"));
        assert!(!needs_sign_in(401, "bad_credentials"), "a wrong password, at sign-in or on Change password, stays on the form");
        assert!(!needs_sign_in(403, "bad_origin"));
    }

    #[test]
    fn audio_url_points_at_the_api() {
        assert_eq!(audio_url("s1"), "/api/v1/songs/s1/audio");
        assert_eq!(download_url("s1", crate::domain::DownloadFormat::Flac), "/api/v1/songs/s1/download?format=flac");
    }

    #[test]
    fn the_live_limit_names_the_live_playlists() {
        let f = Failure::parse(
            409,
            r#"{"error":{"code":"live_limit","message":"at most 2 playlists can be live at once","playlists":[{"id":"a","name":"Late"},{"id":"b","name":"Run"}]}}"#,
            None,
        );
        assert_eq!(f.text(), "At most 2 playlists can be live at once. Late and Run are live now.");
    }
}
