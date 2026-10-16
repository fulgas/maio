//! A MediaMTX service (SPEC 7): anyone can pull every radio over RTSP, SRT and RTMP.
//! Maio publishes each radio to it, answers its auth hook, and reads and kicks its readers through its control API.

use std::{collections::HashMap, time::Duration};

use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::{
    config::RelayConfig,
    outputs::with_input,
    state::AppState,
    store::{RadioRow, Visibility},
};

/// The user Maio publishes and calls the API as; its password is `AppState.relay_secret`.
pub const USER: &str = "maio";

/// The radio's path on MediaMTX: its id when public, its secret key when private.
pub fn path(row: &RadioRow) -> String {
    match row.visibility {
        Visibility::Public => format!("radios/{}", row.id),
        Visibility::Private => format!("listen/{}", row.listen_key),
    }
}

/// ffmpeg arguments that publish a radio to MediaMTX. AAC, because RTMP needs it and MediaMTX does not transcode.
pub fn publish_args(c: &RelayConfig, path: &str, secret: &str) -> Vec<String> {
    let (scheme, rest) = c.publish.split_once("://").unwrap_or(("rtsp", c.publish.as_str()));
    let url = format!("{scheme}://{USER}:{secret}@{}/{path}", rest.trim_end_matches('/'));
    with_input(&["-c:a", "aac", "-b:a", "128k", "-f", "rtsp", "-rtsp_transport", "tcp", &url])
}

/// The pull addresses listeners use.
pub fn addresses(c: &RelayConfig, path: &str) -> Map<String, Value> {
    let h = &c.host;
    let mut m = Map::new();
    m.insert("rtsp".into(), json!(format!("rtsp://{h}:{}/{path}", c.rtsp_port)));
    m.insert("srt".into(), json!(format!("srt://{h}:{}?streamid=read:{path}", c.srt_port)));
    m.insert("rtmp".into(), json!(format!("rtmp://{h}:{}/{path}", c.rtmp_port)));
    m
}

/// What MediaMTX sends to its HTTP auth hook.
#[derive(Debug, Deserialize)]
pub struct AuthRequest {
    #[serde(default)]
    pub user: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub path: String,
}

/// Maio may do anything; anyone may read a current radio's path; nothing else.
pub fn authorize(state: &AppState, r: &AuthRequest) -> bool {
    if r.user == USER {
        return crate::users::sha256_hex(&r.password) == crate::users::sha256_hex(&state.relay_secret);
    }
    r.action == "read" && state.radios().iter().any(|radio| path(&radio.row.lock().unwrap()) == r.path)
}

fn api(state: &AppState, rest: &str) -> String {
    format!("{}/v3/{rest}", state.cfg.relay.api.trim_end_matches('/'))
}

#[derive(Deserialize)]
struct Reader {
    #[serde(rename = "type")]
    kind: String,
    id: String,
}

#[derive(Deserialize)]
struct PathInfo {
    name: String,
    #[serde(default)]
    readers: Vec<Reader>,
}

#[derive(Deserialize)]
struct PathList {
    #[serde(default)]
    items: Vec<PathInfo>,
}

/// Reads MediaMTX's reader counts per path into `relay_readers`.
pub async fn refresh(state: &AppState) {
    let res = state.http.get(api(state, "paths/list?itemsPerPage=1000")).basic_auth(USER, Some(&state.relay_secret)).send().await;
    match res.and_then(|r| r.error_for_status()) {
        Ok(r) => match r.json::<PathList>().await {
            Ok(list) => {
                let counts = list.items.into_iter().map(|p| (p.name, p.readers.len())).collect::<HashMap<_, _>>();
                *state.relay_readers.lock().unwrap() = counts;
            }
            Err(e) => tracing::warn!("relay: unreadable path list: {e}"),
        },
        Err(e) => tracing::debug!("relay: cannot list paths: {e}"),
    }
}

/// Keeps reader counts fresh.
pub async fn watch(state: AppState) {
    loop {
        refresh(&state).await;
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

/// Disconnects everyone reading `path`, after its radio moved to another path.
pub async fn kick(state: AppState, path: String) {
    let res = state.http.get(api(&state, &format!("paths/get/{path}"))).basic_auth(USER, Some(&state.relay_secret)).send().await;
    let Ok(info) = (match res.and_then(|r| r.error_for_status()) {
        Ok(r) => r.json::<PathInfo>().await,
        Err(_) => return, // nobody on it, or MediaMTX is down
    }) else { return };
    for reader in info.readers {
        let kind = match reader.kind.as_str() {
            "rtspSession" => "rtsp/sessions",
            "rtmpConn" => "rtmp/conns",
            "srtConn" => "srt/conns",
            _ => continue,
        };
        let url = api(&state, &format!("{kind}/kick/{}", reader.id));
        if let Err(e) = state.http.post(url).basic_auth(USER, Some(&state.relay_secret)).send().await.and_then(|r| r.error_for_status()) {
            tracing::warn!("relay: cannot kick {} on {path}: {e}", reader.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{RadioRow, Visibility};

    fn cfg() -> RelayConfig {
        RelayConfig { enabled: true, host: "radio.lan".into(), ..RelayConfig::default() }
    }

    fn row(visibility: Visibility) -> RadioRow {
        RadioRow { id: "lobby".into(), name: "Lobby".into(), visibility, listen_key: "K".repeat(32), created_at: String::new() }
    }

    #[test]
    fn public_radios_use_their_id_and_private_ones_their_key() {
        assert_eq!(path(&row(Visibility::Public)), "radios/lobby");
        assert_eq!(path(&row(Visibility::Private)), format!("listen/{}", "K".repeat(32)));
    }

    #[test]
    fn publishing_sends_aac_over_rtsp_with_maio_credentials() {
        let a = publish_args(&cfg(), "radios/main", "s3cret");
        assert!(a.ends_with(&["-c:a", "aac", "-b:a", "128k", "-f", "rtsp", "-rtsp_transport", "tcp", "rtsp://maio:s3cret@mediamtx:8554/radios/main"].map(String::from)), "{a:?}");
    }

    #[test]
    fn addresses_listeners_use() {
        let a = addresses(&cfg(), "radios/main");
        assert_eq!(a["rtsp"], "rtsp://radio.lan:8554/radios/main");
        assert_eq!(a["srt"], "srt://radio.lan:8890?streamid=read:radios/main");
        assert_eq!(a["rtmp"], "rtmp://radio.lan:1935/radios/main");
    }
}
