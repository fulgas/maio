//! The MediaMTX relay (SPEC 9): anyone can pull a live playlist over RTSP, SRT and RTMP. Maio publishes each
//! live playlist to it, answers its auth hook, and reads and kicks its readers through its control API.

use std::{collections::HashMap, sync::Mutex, time::Duration};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::station::args;

/// The user Maio publishes and calls the API as; its password is `Target::secret`.
pub const USER: &str = "maio";

/// Where and how to reach MediaMTX, and the addresses listeners use.
#[derive(Debug, Clone)]
pub struct Target {
    pub publish: String,
    pub api: String,
    pub host: String,
    pub rtsp_port: u16,
    pub srt_port: u16,
    pub rtmp_port: u16,
    /// Random at each start: MediaMTX asks Maio about every action, so nothing is stored.
    pub secret: String,
}

/// ffmpeg arguments that publish a live playlist to MediaMTX. AAC, because RTMP needs it and MediaMTX does not transcode.
pub fn publish_args(t: &Target, path: &str) -> Vec<String> {
    let (scheme, rest) = t.publish.split_once("://").unwrap_or(("rtsp", t.publish.as_str()));
    let url = format!("{scheme}://{USER}:{}@{}/{path}", t.secret, rest.trim_end_matches('/'));
    args(&["-c:a", "aac", "-b:a", "128k", "-f", "rtsp", "-rtsp_transport", "tcp", &url])
}

/// The relay addresses listeners use.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Addresses {
    pub rtsp: String,
    pub srt: String,
    pub rtmp: String,
}

pub fn addresses(t: &Target, path: &str) -> Addresses {
    let h = &t.host;
    Addresses {
        rtsp: format!("rtsp://{h}:{}/{path}", t.rtsp_port),
        srt: format!("srt://{h}:{}?streamid=read:{path}", t.srt_port),
        rtmp: format!("rtmp://{h}:{}/{path}", t.rtmp_port),
    }
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

/// Maio may do anything; anyone may read a live path; nothing else.
pub fn authorize(t: &Target, r: &AuthRequest, live_paths: &[String]) -> bool {
    if r.user == USER {
        return Sha256::digest(r.password.as_bytes()) == Sha256::digest(t.secret.as_bytes());
    }
    r.action == "read" && live_paths.contains(&r.path)
}

/// How many relay readers each path has, refreshed by `watch`.
pub type Readers = Mutex<HashMap<String, usize>>;

fn client() -> reqwest::Client {
    // Building a client only fails if the TLS backend cannot start: a broken build, not a runtime condition.
    reqwest::Client::builder().timeout(Duration::from_secs(5)).build().expect("the HTTP client starts")
}

fn api(t: &Target, rest: &str) -> String {
    format!("{}/v3/{rest}", t.api.trim_end_matches('/'))
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

/// Keeps reader counts fresh, every 5 s.
pub async fn watch(t: Target, readers: std::sync::Arc<Readers>) {
    let http = client();
    loop {
        let res = http.get(api(&t, "paths/list?itemsPerPage=1000")).basic_auth(USER, Some(&t.secret)).send().await;
        match res.and_then(reqwest::Response::error_for_status) {
            Ok(r) => match r.json::<PathList>().await {
                Ok(list) => *readers.lock().unwrap_or_else(|e| e.into_inner()) = list.items.into_iter().map(|p| (p.name, p.readers.len())).collect(),
                Err(e) => tracing::warn!("relay: unreadable path list: {e}"),
            },
            Err(e) => tracing::debug!("relay: cannot list paths: {e}"),
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

/// Disconnects everyone reading `path`, after its playlist moved to another address.
pub async fn kick(t: Target, path: String) {
    let http = client();
    let res = http.get(api(&t, &format!("paths/get/{path}"))).basic_auth(USER, Some(&t.secret)).send().await;
    let Ok(Ok(info)) = (match res.and_then(reqwest::Response::error_for_status) {
        Ok(r) => Ok(r.json::<PathInfo>().await),
        Err(_) => Err(()), // nobody on it, or MediaMTX is down
    }) else {
        return;
    };
    for reader in info.readers {
        let kind = match reader.kind.as_str() {
            "rtspSession" => "rtsp/sessions",
            "rtmpConn" => "rtmp/conns",
            "srtConn" => "srt/conns",
            _ => continue,
        };
        let url = api(&t, &format!("{kind}/kick/{}", reader.id));
        if let Err(e) = http.post(url).basic_auth(USER, Some(&t.secret)).send().await.and_then(reqwest::Response::error_for_status) {
            tracing::warn!("relay: cannot kick {} on {path}: {e}", reader.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> Target {
        Target {
            publish: "rtsp://mediamtx:8554".into(),
            api: "http://mediamtx:9997".into(),
            host: "radio.lan".into(),
            rtsp_port: 8555,
            srt_port: 8891,
            rtmp_port: 1936,
            secret: "s3cret".into(),
        }
    }

    #[test]
    fn publishing_is_aac_over_rtsp_as_maio() {
        let a = publish_args(&target(), "live/p1");
        assert!(a.windows(2).any(|w| w == ["-c:a", "aac"]));
        assert_eq!(a.last().unwrap(), "rtsp://maio:s3cret@mediamtx:8554/live/p1");
    }

    #[test]
    fn addresses_use_the_public_host_and_ports() {
        let a = addresses(&target(), "listen/KEY");
        assert_eq!(a.rtsp, "rtsp://radio.lan:8555/listen/KEY");
        assert_eq!(a.srt, "srt://radio.lan:8891?streamid=read:listen/KEY");
        assert_eq!(a.rtmp, "rtmp://radio.lan:1936/listen/KEY");
    }

    #[test]
    fn maio_may_do_anything_and_anyone_may_read_a_live_path() {
        let t = target();
        let live = vec!["live/p1".to_string()];
        let req = |user: &str, password: &str, action: &str, path: &str| AuthRequest {
            user: user.into(),
            password: password.into(),
            action: action.into(),
            path: path.into(),
        };
        assert!(authorize(&t, &req(USER, "s3cret", "publish", "live/p1"), &live));
        assert!(!authorize(&t, &req(USER, "wrong", "publish", "live/p1"), &live));
        assert!(authorize(&t, &req("", "", "read", "live/p1"), &live));
        assert!(!authorize(&t, &req("", "", "read", "live/p2"), &live), "not live");
        assert!(!authorize(&t, &req("", "", "publish", "live/p1"), &live), "only Maio publishes");
    }
}
