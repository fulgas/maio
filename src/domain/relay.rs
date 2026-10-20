//! MediaMTX as Maio last saw it (SPEC 9): what the header's MediaMTX panel shows.

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
