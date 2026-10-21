//! A playlist's live state as the API sends it (arc42 3.5): settings, what plays, and the addresses.

use serde::Serialize;

use crate::{
    adapters::live::relay,
    domain::{Visibility, live},
    usecases::{LiveView, ports::OnAir},
    web::config::Serve,
};

#[derive(Debug, Serialize, Default)]
pub struct Addresses {
    pub mp3: Option<String>,
    pub aac: Option<String>,
    pub hls: Option<String>,
    pub rtsp: Option<String>,
    pub srt: Option<String>,
    pub rtmp: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LiveJson {
    pub live: bool,
    pub shuffle: bool,
    pub visibility: Visibility,
    pub on_air: Option<OnAir>,
    pub live_count: usize,
    pub max_live: usize,
    /// Only while live: where to listen.
    pub addresses: Option<Addresses>,
}

pub fn json(v: LiveView, serve: &Serve, relay: Option<&relay::Target>) -> LiveJson {
    let p = &v.playlist;
    let addresses = p.live.on.then(|| {
        let base = live::stream_base(p);
        let r = relay.map(|t| relay::addresses(t, &live::relay_path(p)));
        Addresses {
            mp3: serve.mp3.then(|| format!("{base}/stream.mp3")),
            aac: serve.aac.then(|| format!("{base}/stream.aac")),
            hls: serve.hls.then(|| format!("{base}/hls/index.m3u8")),
            rtsp: r.as_ref().map(|a| a.rtsp.clone()),
            srt: r.as_ref().map(|a| a.srt.clone()),
            rtmp: r.as_ref().map(|a| a.rtmp.clone()),
        }
    });
    LiveJson {
        live: p.live.on,
        shuffle: p.live.shuffle,
        visibility: p.live.visibility,
        on_air: v.on_air,
        live_count: v.live_count,
        max_live: v.max_live,
        addresses,
    }
}
