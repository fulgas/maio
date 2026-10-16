use std::{
    path::Path,
    sync::Arc,
    process::Stdio,
    time::{Duration, Instant},
};

use bytes::Bytes;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::{broadcast, watch},
};

use crate::{
    config::{Codec, OutputConfig, OutputKind},
    radio::Radio,
    state::{AppState, OutputStatus},
};

const INPUT: [&str; 11] = ["-hide_banner", "-v", "warning", "-f", "s16le", "-ar", "48000", "-ac", "2", "-i", "pipe:0"];

pub(crate) fn with_input(rest: &[&str]) -> Vec<String> {
    INPUT.iter().chain(rest).map(|s| s.to_string()).collect()
}

pub fn push_args(o: &OutputConfig) -> Vec<String> {
    let codec = match o.codec {
        Codec::Opus => "libopus",
        Codec::Aac => "aac",
        Codec::Mp3 => "libmp3lame",
    };
    let bitrate = format!("{}k", o.bitrate_k);
    let format: &[&str] = match (o.kind, o.codec) {
        (OutputKind::Rtsp, _) => &["-f", "rtsp", "-rtsp_transport", "tcp"],
        (OutputKind::Srt, _) => &["-f", "mpegts"],
        (OutputKind::Rtmp, _) => &["-f", "flv"],
        (OutputKind::Icecast, Codec::Mp3) => &["-f", "mp3", "-content_type", "audio/mpeg"],
        (OutputKind::Icecast, Codec::Opus) => &["-f", "ogg", "-content_type", "audio/ogg"],
        (OutputKind::Icecast, Codec::Aac) => &["-f", "adts", "-content_type", "audio/aac"],
    };
    let mut a = with_input(&["-c:a", codec, "-b:a", &bitrate]);
    a.extend(format.iter().map(|s| s.to_string()));
    a.push(o.url.clone());
    a
}

pub fn mp3_args() -> Vec<String> {
    with_input(&["-c:a", "libmp3lame", "-b:a", "160k", "-f", "mp3", "pipe:1"])
}

/// Raw AAC in ADTS frames: each frame carries its own header, so listeners can join at any moment.
pub fn aac_args() -> Vec<String> {
    with_input(&["-c:a", "aac", "-b:a", "128k", "-f", "adts", "pipe:1"])
}

pub fn hls_args(dir: &Path) -> Vec<String> {
    let playlist = dir.join("index.m3u8");
    with_input(&["-c:a", "aac", "-b:a", "128k", "-f", "hls", "-hls_time", "4", "-hls_list_size", "6",
                 "-hls_flags", "delete_segments", &playlist.to_string_lossy()])
}

/// Hides credentials in a URL: `icecast://source:secret@host` becomes `icecast://***@host`,
/// and secret query values (SRT `passphrase`, `streamid`, any key with pass, key, token or secret) become `***`.
pub fn redact(url: &str) -> String {
    let url = match (url.find("://"), url.rfind('@')) {
        (Some(s), Some(at)) if at > s => format!("{}://***@{}", &url[..s], &url[at + 1..]),
        _ => url.to_string(),
    };
    let Some((base, query)) = url.split_once('?') else { return url };
    let secret = |k: &str| {
        let k = k.to_ascii_lowercase();
        k == "streamid" || ["pass", "key", "token", "secret"].iter().any(|w| k.contains(w))
    };
    let query = query
        .split('&')
        .map(|kv| match kv.split_once('=') {
            Some((k, _)) if secret(k) => format!("{k}=***"),
            _ => kv.to_string(),
        })
        .collect::<Vec<_>>()
        .join("&");
    format!("{base}?{query}")
}

/// Redacts every URL in an ffmpeg log line, since ffmpeg prints the raw output URL on errors.
pub fn scrub_line(line: &str) -> String {
    line.split(' ').map(|w| if w.contains("://") { redact(w) } else { w.to_string() }).collect::<Vec<_>>().join(" ")
}

fn set_state(state: &AppState, r: &Radio, name: &str, value: &'static str) {
    let changed = {
        let mut outs = r.outputs.lock().unwrap();
        match outs.iter_mut().find(|o| o.name == name) {
            Some(o) if o.state != value => {
                o.state = value;
                Some(o.clone())
            }
            _ => None,
        }
    };
    if let Some(o) = changed {
        state.emit(Some(&r.id), "output", serde_json::to_value(o).expect("output serialises"));
    }
}

/// Starts one radio's push outputs (those naming it in config) and its pull encoders.
pub fn spawn_for(state: &AppState, r: &Arc<Radio>) {
    for (i, o) in state.cfg.outputs.iter().enumerate().filter(|(_, o)| o.radio == r.id) {
        let kind = format!("{:?}", o.kind).to_lowercase();
        spawn_encoder(state, r, format!("{kind}-{i}"), kind, redact(&o.url), push_args(o), None);
    }
    let base = format!("/radios/{}", r.id);
    if state.cfg.serve.mp3 {
        spawn_encoder(state, r, "mp3".into(), "pull".into(), format!("{base}/stream.mp3"), mp3_args(), Some(r.mp3.clone()));
    }
    if state.cfg.serve.aac {
        spawn_encoder(state, r, "aac".into(), "pull".into(), format!("{base}/stream.aac"), aac_args(), Some(r.aac.clone()));
    }
    if state.cfg.serve.hls {
        spawn_encoder(state, r, "hls".into(), "pull".into(), format!("{base}/hls/index.m3u8"), hls_args(&state.hls_dir(&r.id)), None);
    }
    if state.cfg.relay.enabled {
        // Publishes to MediaMTX on the radio's current path; a new key or visibility restarts it on the new path.
        let (relay, secret) = (state.cfg.relay.clone(), state.relay_secret.clone());
        let radio = r.clone();
        let args: Args = Arc::new(move || crate::relay::publish_args(&relay, &crate::relay::path(&radio.row.lock().unwrap()), &secret));
        r.outputs.lock().unwrap().push(OutputStatus { name: "relay".into(), kind: "relay".into(), target: "mediamtx".into(), state: "starting" });
        tokio::spawn(run_encoder(state.clone(), r.clone(), "relay".into(), args, None, Some(r.access.subscribe())));
    }
}

/// Builds an encoder's ffmpeg arguments each time it starts.
type Args = Arc<dyn Fn() -> Vec<String> + Send + Sync>;

pub fn spawn_encoder(state: &AppState, r: &Arc<Radio>, name: String, kind: String, target: String, args: Vec<String>, stdout_to: Option<broadcast::Sender<Bytes>>) {
    r.outputs.lock().unwrap().push(OutputStatus { name: name.clone(), kind, target, state: "starting" });
    tokio::spawn(run_encoder(state.clone(), r.clone(), name, Arc::new(move || args.clone()), stdout_to, None));
}

/// One ffmpeg fed from the PCM bus, restarted with backoff when it exits.
/// A slow or dead output only lags its own receiver: it never blocks playout or other outputs.
/// With `restart`, a bump kills ffmpeg and starts it again at once with fresh arguments.
async fn run_encoder(state: AppState, r: Arc<Radio>, name: String, args: Args, stdout_to: Option<broadcast::Sender<Bytes>>, mut restart: Option<watch::Receiver<u64>>) {
    let backoff = [1u64, 5, 30];
    let mut fails = 0usize;
    let mut closed = r.closed.subscribe();
    loop {
        let mut restarted = false;
        if r.is_closed() {
            return;
        }
        set_state(&state, &r, &name, "connecting");
        let started = Instant::now();
        let spawned = Command::new("ffmpeg")
            .args(args())
            .stdin(Stdio::piped())
            .stdout(if stdout_to.is_some() { Stdio::piped() } else { Stdio::null() })
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn();
        match spawned {
            Ok(mut child) => {
                let mut stdin = child.stdin.take().expect("piped stdin");
                if let Some(err) = child.stderr.take() {
                    let name = name.clone();
                    tokio::spawn(async move {
                        let mut lines = BufReader::new(err).lines();
                        while let Ok(Some(line)) = lines.next_line().await {
                            tracing::warn!(output = %name, "ffmpeg: {}", scrub_line(&line));
                        }
                    });
                }
                if let (Some(tx), Some(mut out)) = (stdout_to.clone(), child.stdout.take()) {
                    tokio::spawn(async move {
                        let mut buf = vec![0u8; 4096];
                        while let Ok(n) = out.read(&mut buf).await {
                            if n == 0 {
                                break;
                            }
                            let _ = tx.send(Bytes::copy_from_slice(&buf[..n]));
                        }
                    });
                }
                set_state(&state, &r, &name, "up");
                let mut rx = r.pcm.subscribe();
                loop {
                    let msg = tokio::select! {
                        m = rx.recv() => m,
                        _ = closed.changed() => {
                            let _ = child.kill().await;
                            return;
                        }
                        _ = async {
                            match restart.as_mut() {
                                Some(w) => { let _ = w.changed().await; }
                                None => std::future::pending::<()>().await,
                            }
                        } => {
                            restarted = true;
                            break;
                        }
                    };
                    match msg {
                        Ok(chunk) => {
                            if stdin.write_all(&chunk).await.is_err() {
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(broadcast::error::RecvError::Closed) => return,
                    }
                }
                let _ = child.kill().await;
            }
            Err(e) => tracing::error!(output = %name, "cannot start ffmpeg: {e}"),
        }
        if restarted {
            continue;
        }
        set_state(&state, &r, &name, "down");
        if started.elapsed() > Duration::from_secs(60) {
            fails = 0;
        }
        let wait = backoff[fails.min(backoff.len() - 1)];
        fails += 1;
        tokio::time::sleep(Duration::from_secs(wait)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(kind: OutputKind, codec: Codec, url: &str) -> OutputConfig {
        OutputConfig { kind, codec, url: url.into(), bitrate_k: 160, radio: "main".into() }
    }

    #[test]
    fn rtsp_opus_arguments() {
        let a = push_args(&out(OutputKind::Rtsp, Codec::Opus, "rtsp://host.docker.internal:8554/music"));
        assert_eq!(a[..11], ["-hide_banner", "-v", "warning", "-f", "s16le", "-ar", "48000", "-ac", "2", "-i", "pipe:0"]);
        assert_eq!(a[11..], ["-c:a", "libopus", "-b:a", "160k", "-f", "rtsp", "-rtsp_transport", "tcp", "rtsp://host.docker.internal:8554/music"]);
    }

    #[test]
    fn icecast_and_srt_arguments() {
        let a = push_args(&out(OutputKind::Icecast, Codec::Mp3, "icecast://source:pw@radio:8000/live"));
        assert!(a.ends_with(&["-f".into(), "mp3".into(), "-content_type".into(), "audio/mpeg".into(), "icecast://source:pw@radio:8000/live".into()]));
        let s = push_args(&out(OutputKind::Srt, Codec::Aac, "srt://x:9000"));
        assert!(s.contains(&"mpegts".to_string()) && s.contains(&"aac".to_string()));
    }

    #[test]
    fn redacts_query_secrets() {
        assert_eq!(redact("srt://h:9000?passphrase=abc123&latency=200"), "srt://h:9000?passphrase=***&latency=200");
        assert_eq!(redact("srt://h:9000?streamid=publish:music&mode=caller"), "srt://h:9000?streamid=***&mode=caller");
    }

    #[test]
    fn scrubs_urls_in_ffmpeg_log_lines() {
        let line = "Error opening output icecast://source:hunter2@127.0.0.1:9/live: Connection refused";
        let out = scrub_line(line);
        assert!(!out.contains("hunter2"), "{out}");
        assert!(out.contains("icecast://***@127.0.0.1:9/live:"));
    }

    #[test]
    fn aac_pull_arguments() {
        let a = aac_args();
        assert!(a.ends_with(&["-c:a".into(), "aac".into(), "-b:a".into(), "128k".into(), "-f".into(), "adts".into(), "pipe:1".into()]), "{a:?}");
    }

    #[test]
    fn redacts_credentials() {
        assert_eq!(redact("icecast://source:pw@radio:8000/live"), "icecast://***@radio:8000/live");
        assert_eq!(redact("rtsp://host:8554/music"), "rtsp://host:8554/music");
    }
}
