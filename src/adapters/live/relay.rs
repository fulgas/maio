//! The MediaMTX relay (arc42 8.4): anyone can pull a live playlist over RTSP, SRT and RTMP. Maio publishes each
//! live playlist to it, answers its auth hook, and reads and kicks its readers through its control API.

use std::{collections::HashMap, sync::Mutex, time::Duration};

use chrono::{DateTime, Utc};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::station::args;
use crate::domain::{
    PlaylistRef,
    relay::{Caller, RelayAddresses, RelayReaders, RelayState, RelayStatus, RelayStream},
};

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
pub fn addresses(t: &Target, path: &str) -> RelayAddresses {
    let h = &t.host;
    RelayAddresses {
        rtsp: format!("rtsp://{h}:{}/{path}", t.rtsp_port),
        srt: format!("srt://{h}:{}?streamid=read:{path}", t.srt_port),
        rtmp: format!("rtmp://{h}:{}/{path}", t.rtmp_port),
    }
}

/// Who is asking the auth hook: Maio's user with its secret (compared by digest, in constant time for its length),
/// that user with anything else, or anyone. What each may do is the domain's rule (`relay::allowed`).
pub fn caller(t: &Target, user: &str, password: &str) -> Caller {
    match user {
        USER if Sha256::digest(password.as_bytes()) == Sha256::digest(t.secret.as_bytes()) => Caller::Maio,
        USER => Caller::Impostor,
        _ => Caller::Anyone,
    }
}

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
#[serde(rename_all = "camelCase")]
struct PathInfo {
    name: String,
    #[serde(default)]
    readers: Vec<Reader>,
    #[serde(default)]
    ready: bool,
    #[serde(default)]
    ready_time: Option<String>,
    #[serde(default)]
    tracks2: Vec<Track>,
    #[serde(default)]
    bytes_sent: u64,
    #[serde(default)]
    inbound_frames_in_error: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Track {
    codec: String,
    #[serde(default)]
    codec_props: CodecProps,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct CodecProps {
    sample_rate: Option<u32>,
    channel_count: Option<u32>,
}

#[derive(Deserialize)]
struct Info {
    version: String,
    started: String,
}

/// MediaMTX as the last check found it.
#[derive(Debug, Clone, Default)]
pub struct Seen {
    /// A check has run since Maio started.
    checked: bool,
    /// The last check got an answer; then `version` is set.
    version: Option<String>,
    started: Option<DateTime<Utc>>,
    paths: HashMap<String, PathSeen>,
}

#[derive(Debug, Clone, Default)]
struct PathSeen {
    ready_since: Option<DateTime<Utc>>,
    ready: bool,
    format: Option<String>,
    readers: RelayReaders,
    bytes_sent: u64,
    frames_in_error: u64,
}

impl Seen {
    fn from_api(info: Info, paths: PathList) -> Self {
        let paths = paths
            .items
            .into_iter()
            .map(|p| {
                let mut readers = RelayReaders::default();
                for r in &p.readers {
                    match r.kind.as_str() {
                        "rtspSession" | "rtspsSession" => readers.rtsp += 1,
                        "srtConn" => readers.srt += 1,
                        "rtmpConn" | "rtmpsConn" => readers.rtmp += 1,
                        _ => {}
                    }
                }
                let format = p.tracks2.first().map(|t| audio(&t.codec, t.codec_props.sample_rate, t.codec_props.channel_count));
                let seen = PathSeen {
                    ready_since: p.ready_time.as_deref().and_then(|t| t.parse().ok()),
                    ready: p.ready,
                    format,
                    readers,
                    bytes_sent: p.bytes_sent,
                    frames_in_error: p.inbound_frames_in_error,
                };
                (p.name, seen)
            })
            .collect();
        Self { checked: true, version: Some(info.version), started: info.started.parse().ok(), paths }
    }

    /// Everyone reading `path` through MediaMTX, for the listener count.
    pub fn readers(&self, path: &str) -> usize {
        self.paths.get(path).map_or(0, |p| p.readers.rtsp + p.readers.srt + p.readers.rtmp)
    }
}

/// "AAC 48 kHz stereo", from MediaMTX's track description.
fn audio(codec: &str, rate: Option<u32>, channels: Option<u32>) -> String {
    let name = match codec {
        "MPEG-4 Audio" => "AAC".to_string(),
        "MPEG-1/2 Audio" => "MP3".to_string(),
        other => other.to_string(),
    };
    let rate = rate.map(|r| if r % 1000 == 0 { format!("{} kHz", r / 1000) } else { format!("{:.1} kHz", f64::from(r) / 1000.0) });
    let channels = channels.map(|c| match c {
        1 => "mono".to_string(),
        2 => "stereo".to_string(),
        n => format!("{n} channels"),
    });
    [Some(name), rate, channels].into_iter().flatten().collect::<Vec<_>>().join(" ")
}

fn seconds_since(t: DateTime<Utc>, now: DateTime<Utc>) -> u64 {
    ((now - t).num_milliseconds().max(0) as f64 / 1000.0).round() as u64
}

/// What the MediaMTX panel shows: MediaMTX itself, then each live playlist (`live`: the playlist and its relay path).
pub fn status(seen: &Seen, t: &Target, live: &[(PlaylistRef, String)], now: DateTime<Utc>) -> RelayStatus {
    let state = match (seen.checked, &seen.version) {
        (false, _) => RelayState::Checking,
        (true, Some(_)) => RelayState::Up,
        (true, None) => RelayState::Down,
    };
    let streams = live
        .iter()
        .map(|(playlist, path)| {
            let p = seen.paths.get(path).cloned().unwrap_or_default();
            RelayStream {
                playlist: playlist.clone(),
                arriving: p.ready,
                arriving_s: p.ready.then_some(p.ready_since).flatten().map(|s| seconds_since(s, now)),
                format: p.format,
                readers: p.readers,
                bytes_sent: p.bytes_sent,
                frames_in_error: p.frames_in_error,
            }
        })
        .collect();
    RelayStatus {
        state,
        version: seen.version.clone(),
        up_s: seen.started.map(|s| seconds_since(s, now)),
        rtsp: format!("{}:{}", t.host, t.rtsp_port),
        srt: format!("{}:{}", t.host, t.srt_port),
        rtmp: format!("{}:{}", t.host, t.rtmp_port),
        streams,
    }
}

#[derive(Deserialize)]
struct PathList {
    #[serde(default)]
    items: Vec<PathInfo>,
}

/// Asks MediaMTX how it is and what it carries, every 5 s.
pub async fn watch(t: Target, seen: std::sync::Arc<Mutex<Seen>>) {
    let http = client();
    loop {
        let now = match check(&http, &t).await {
            Ok(s) => s,
            Err(e) => {
                tracing::debug!("relay: no answer from MediaMTX: {e}");
                Seen { checked: true, ..Seen::default() }
            }
        };
        *seen.lock().unwrap_or_else(|e| e.into_inner()) = now;
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn check(http: &reqwest::Client, t: &Target) -> reqwest::Result<Seen> {
    let get = |rest: &str| http.get(api(t, rest)).basic_auth(USER, Some(&t.secret)).send();
    let info = get("info").await?.error_for_status()?.json::<Info>().await?;
    let paths = get("paths/list?itemsPerPage=1000").await?.error_for_status()?.json::<PathList>().await?;
    Ok(Seen::from_api(info, paths))
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

    // As MediaMTX 1.21.2 answers (trimmed).
    const INFO: &str = r#"{"version":"v1.21.2","started":"2026-10-20T07:00:00.97Z"}"#;
    const PATHS: &str = r#"{"items":[{"name":"listen/KEY","ready":true,"readyTime":"2026-10-20T07:58:00.02Z",
        "tracks2":[{"codec":"MPEG-4 Audio","codecProps":{"sampleRate":48000,"channelCount":2}}],
        "readers":[{"type":"rtmpConn","id":"a"},{"type":"rtspSession","id":"b"},{"type":"rtspSession","id":"c"}],
        "bytesSent":12300000,"inboundFramesInError":2}]}"#;

    fn now() -> chrono::DateTime<chrono::Utc> {
        "2026-10-20T08:10:00Z".parse().unwrap()
    }

    fn checked() -> Seen {
        Seen::from_api(serde_json::from_str(INFO).unwrap(), serde_json::from_str(PATHS).unwrap())
    }

    fn live(path: &str) -> (PlaylistRef, String) {
        (PlaylistRef { id: "p1".into(), name: "Late night coding".into() }, path.into())
    }

    #[test]
    fn a_check_reads_mediamtx_and_each_live_playlist_on_it() {
        let s = status(&checked(), &target(), &[live("listen/KEY")], now());
        assert_eq!((s.state, s.version.as_deref(), s.up_s), (RelayState::Up, Some("v1.21.2"), Some(4199)));
        assert_eq!((s.rtsp.as_str(), s.srt.as_str(), s.rtmp.as_str()), ("radio.lan:8555", "radio.lan:8891", "radio.lan:1936"));
        let one = &s.streams[0];
        assert_eq!((one.playlist.name.as_str(), one.arriving, one.arriving_s), ("Late night coding", true, Some(720)));
        assert_eq!(one.format.as_deref(), Some("AAC 48 kHz stereo"));
        assert_eq!(one.readers, RelayReaders { rtsp: 2, srt: 0, rtmp: 1 });
        assert_eq!((one.bytes_sent, one.frames_in_error), (12_300_000, 2));
        assert_eq!(checked().readers("listen/KEY"), 3, "the listener count adds them up");
    }

    #[test]
    fn a_live_playlist_missing_on_mediamtx_is_not_arriving() {
        let s = status(&checked(), &target(), &[live("live/p1")], now());
        assert_eq!((s.streams[0].arriving, s.streams[0].readers.rtsp), (false, 0));
    }

    #[test]
    fn before_the_first_check_it_is_checking_and_after_a_failed_one_down() {
        assert_eq!(status(&Seen::default(), &target(), &[], now()).state, RelayState::Checking);
        let down = Seen { checked: true, ..Seen::default() };
        let s = status(&down, &target(), &[live("listen/KEY")], now());
        assert_eq!((s.state, s.version, s.streams[0].arriving), (RelayState::Down, None, false));
    }

    #[test]
    fn audio_formats_read_like_people_say_them() {
        assert_eq!(audio("MPEG-4 Audio", Some(48000), Some(2)), "AAC 48 kHz stereo");
        assert_eq!(audio("Opus", Some(44100), Some(1)), "Opus 44.1 kHz mono");
        assert_eq!(audio("MPEG-1/2 Audio", None, None), "MP3");
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
    fn the_secret_tells_maio_from_an_impostor_and_anyone() {
        let t = target();
        assert_eq!(caller(&t, USER, "s3cret"), Caller::Maio);
        assert_eq!(caller(&t, USER, "wrong"), Caller::Impostor);
        assert_eq!(caller(&t, "", ""), Caller::Anyone);
        assert_eq!(caller(&t, "someone", "s3cret"), Caller::Anyone);
    }
}
