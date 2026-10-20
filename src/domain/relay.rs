//! MediaMTX as Maio last saw it (arc42 8.4): what the header's MediaMTX panel shows.

use serde::{Deserialize, Serialize};

use super::PlaylistRef;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RelayState {
    /// No answer yet since Maio started.
    Checking,
    Up,
    /// Its control API did not answer the last check.
    Down,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelayStatus {
    pub state: RelayState,
    pub version: Option<String>,
    /// How long MediaMTX has been running.
    pub up_s: Option<u64>,
    /// Where listeners reach it, `host:port`.
    pub rtsp: String,
    pub srt: String,
    pub rtmp: String,
    /// One per live playlist.
    pub streams: Vec<RelayStream>,
}

/// A live playlist on MediaMTX.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelayStream {
    pub playlist: PlaylistRef,
    /// Maio's audio reaches MediaMTX.
    pub arriving: bool,
    pub arriving_s: Option<u64>,
    /// "AAC 48 kHz stereo".
    pub format: Option<String>,
    pub readers: RelayReaders,
    pub bytes_sent: u64,
    pub frames_in_error: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayReaders {
    pub rtsp: usize,
    pub srt: usize,
    pub rtmp: usize,
}

/// The listener addresses of a live playlist on MediaMTX.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayAddresses {
    pub rtsp: String,
    pub srt: String,
    pub rtmp: String,
}

/// Who is asking MediaMTX's auth hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Caller {
    /// Maio itself, with the secret it made at start.
    Maio,
    /// Maio's user name with the wrong secret.
    Impostor,
    Anyone,
}

/// Maio may do anything; anyone may read a live path; nothing else.
pub fn allowed(caller: Caller, action: &str, path: &str, live_paths: &[String]) -> bool {
    match caller {
        Caller::Maio => true,
        Caller::Impostor => false,
        Caller::Anyone => action == "read" && live_paths.iter().any(|p| p == path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maio_may_do_anything_and_anyone_may_read_a_live_path() {
        let live = vec!["live/p1".to_string()];
        assert!(allowed(Caller::Maio, "publish", "live/p1", &live));
        assert!(!allowed(Caller::Impostor, "read", "live/p1", &live), "a wrong secret reads nothing");
        assert!(allowed(Caller::Anyone, "read", "live/p1", &live));
        assert!(!allowed(Caller::Anyone, "read", "live/p2", &live), "not live");
        assert!(!allowed(Caller::Anyone, "publish", "live/p1", &live), "only Maio publishes");
    }
}
