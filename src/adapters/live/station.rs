//! One live playlist on air (SPEC 9): decode the next song, crossfade, pace 20 ms chunks onto the PCM bus;
//! encoders turn the bus into MP3, AAC and HLS bytes and publish AAC to the relay.

use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use bytes::Bytes;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
    sync::{broadcast, watch},
};

use super::mixer::{CHANNELS, CHUNK, Deck, MixEvent, Mixer, SAMPLE_RATE};
use crate::{
    domain::{Playlist, live::PlayOrder},
    usecases::ports::LiveSong,
};

/// Start decoding the next song this long before the current one starts fading.
const LOAD_AHEAD_S: f32 = 15.0;

/// What the playout loop reads each tick: the playlist as it is now.
pub struct Setlist {
    pub songs: Vec<LiveSong>,
    pub shuffle: bool,
}

pub struct Station {
    pub id: String,
    pub data_dir: PathBuf,
    pub setlist: Mutex<Setlist>,
    pub now: Mutex<Option<LiveSong>>,
    pub next: Mutex<Option<LiveSong>>,
    /// 20 ms PCM chunks; encoders subscribe.
    pub pcm: broadcast::Sender<Bytes>,
    pub mp3: broadcast::Sender<Bytes>,
    pub aac: broadcast::Sender<Bytes>,
    pub listeners: Arc<AtomicUsize>,
    /// Bumped when the address changes: open streams end, the relay republishes on the new path.
    pub moved: watch::Sender<u64>,
    /// The address streams were opened on (`stream_base`); a change bumps `moved`.
    pub address: Mutex<String>,
    pub closed: watch::Sender<bool>,
    /// Set when MediaMTX relays this station: a move kicks the readers of the old path.
    pub relay: Option<super::relay::Target>,
}

impl Station {
    pub fn new(p: &Playlist, songs: &[LiveSong], data_dir: PathBuf, relay: Option<super::relay::Target>) -> Arc<Self> {
        Arc::new(Self {
            id: p.id.clone(),
            data_dir,
            setlist: Mutex::new(Setlist { songs: songs.to_vec(), shuffle: p.live.shuffle }),
            now: Mutex::new(None),
            next: Mutex::new(None),
            pcm: broadcast::channel(256).0,
            // ponytail: 512 chunks of backlog per bus; a listener further behind is dropped (lagged).
            mp3: broadcast::channel(512).0,
            aac: broadcast::channel(512).0,
            listeners: Arc::new(AtomicUsize::new(0)),
            moved: watch::channel(0).0,
            address: Mutex::new(crate::domain::live::stream_base(p)),
            closed: watch::channel(false).0,
            relay,
        })
    }

    /// Its path on the relay now (`live/{id}` or `listen/{key}`).
    pub fn relay_path(&self) -> String {
        self.address.lock().unwrap().trim_start_matches('/').to_string()
    }

    /// Takes the playlist as it is now; a new address ends open streams.
    pub fn update(&self, p: &Playlist, songs: &[LiveSong]) {
        *self.setlist.lock().unwrap() = Setlist { songs: songs.to_vec(), shuffle: p.live.shuffle };
        let address = crate::domain::live::stream_base(p);
        let mut current = self.address.lock().unwrap();
        if *current != address {
            if let Some(t) = self.relay.clone() {
                tokio::spawn(super::relay::kick(t, current.trim_start_matches('/').to_string()));
            }
            *current = address;
            self.moved.send_modify(|n| *n += 1);
        }
    }

    pub fn close(&self) {
        self.closed.send_replace(true);
        self.moved.send_modify(|n| *n += 1);
    }

    fn is_closed(&self) -> bool {
        *self.closed.borrow()
    }
}

/// Decodes a song while it plays: one running ffmpeg, its PCM sent in slices through a bounded channel.
/// A full channel blocks the pipe, so memory stays at about 64 slices (~512 KB) whatever the song's length.
/// A file ffmpeg cannot read closes the channel with no audio (`Deck::is_dead`).
fn decode(song: &LiveSong, data_dir: &Path) -> Deck {
    let total = (song.seconds.max(0.0) * (SAMPLE_RATE * CHANNELS) as f64) as usize;
    let (tx, rx) = tokio::sync::mpsc::channel::<Vec<i16>>(64);
    let path = data_dir.join(&song.file);
    let id = song.id.clone();
    tokio::spawn(async move {
        let child = Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(&path)
            .args(["-f", "s16le", "-ar", "48000", "-ac", "2", "pipe:1"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn();
        let Ok(mut child) = child else { return };
        let Some(mut out) = child.stdout.take() else { return };
        let mut bytes = vec![0u8; super::mixer::SLICE * 2];
        let mut carry: Option<u8> = None;
        loop {
            let n = match out.read(&mut bytes).await {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            let mut raw: Vec<u8> = carry.take().into_iter().chain(bytes[..n].iter().copied()).collect();
            if raw.len() % 2 == 1 {
                carry = raw.pop(); // half a sample: keep it for the next read
            }
            let slice: Vec<i16> = raw.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
            if tx.send(slice).await.is_err() {
                break; // the deck was dropped (off air, or skipped): stop decoding
            }
        }
        let _ = child.kill().await;
        tracing::debug!(song = %id, "decoder finished");
    });
    Deck::new(song.id.clone(), total, rx)
}

fn seed() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64)
}

/// The playout loop: one 20 ms chunk per tick, paced against a fixed start so it never drifts.
pub async fn playout(st: Arc<Station>, crossfade_s: f32) {
    let mut mixer = Mixer::new(crossfade_s);
    let mut order = PlayOrder::default();
    let mut buf = vec![0i16; CHUNK];
    let mut events = Vec::new();
    // The next song's deck, decoding ahead; it moves to the mixer once it has audio.
    let mut loading: Option<Deck> = None;
    // The song picked to play next: shown as "next" at once, decoded only near the end of the current one.
    let mut queued: Option<LiveSong> = None;
    let mut fails: u32 = 0;
    let mut next_try = tokio::time::Instant::now();
    let mut last_picked: Option<String> = None;
    let start = tokio::time::Instant::now();
    let mut n: u64 = 0;
    loop {
        if st.is_closed() {
            return;
        }
        let now = tokio::time::Instant::now();
        if queued.is_none() && mixer.next.is_none() && loading.is_none() && now >= next_try {
            let (ids, shuffle) = {
                let s = st.setlist.lock().unwrap();
                (s.songs.iter().map(|x| x.id.clone()).collect::<Vec<_>>(), s.shuffle)
            };
            match order.next(&ids, shuffle, last_picked.as_deref(), seed()) {
                None => next_try = now + Duration::from_secs(1), // empty: silence, look again in a second
                Some(id) => {
                    last_picked = Some(id.clone());
                    let song = st.setlist.lock().unwrap().songs.iter().find(|s| s.id == id).cloned();
                    if let Some(song) = song {
                        *st.next.lock().unwrap() = Some(song.clone());
                        queued = Some(song);
                    }
                }
            }
        }
        if mixer.remaining_s() <= mixer.fade_s() + LOAD_AHEAD_S
            && let Some(song) = queued.take()
        {
            loading = Some(decode(&song, &st.data_dir));
        }
        if let Some(deck) = loading.as_mut() {
            if deck.is_dead() {
                tracing::warn!(station = %st.id, song = %deck.id, "cannot decode a live song; skipping it");
                loading = None;
                // A playlist of only broken files must not spin: wait longer after each failure in a row.
                fails += 1;
                next_try = tokio::time::Instant::now() + Duration::from_millis(200 * u64::from(fails.min(25)));
            } else if deck.is_ready() {
                fails = 0;
                mixer.next = loading.take();
            }
        }
        mixer.render(&mut buf, &mut events);
        for e in events.drain(..) {
            if let MixEvent::Started(id) = e {
                let song = st
                    .next
                    .lock()
                    .unwrap()
                    .take()
                    .filter(|s| s.id == id)
                    .or_else(|| st.setlist.lock().unwrap().songs.iter().find(|s| s.id == id).cloned());
                *st.now.lock().unwrap() = song;
            }
        }
        let mut bytes = Vec::with_capacity(CHUNK * 2);
        for s in &buf {
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        let _ = st.pcm.send(Bytes::from(bytes));
        n += 1;
        tokio::time::sleep_until(start + Duration::from_millis(20 * n)).await;
    }
}

const INPUT: [&str; 11] = ["-hide_banner", "-v", "warning", "-f", "s16le", "-ar", "48000", "-ac", "2", "-i", "pipe:0"];

pub fn args(rest: &[&str]) -> Vec<String> {
    INPUT.iter().chain(rest).map(|s| (*s).to_string()).collect()
}

pub fn mp3_args() -> Vec<String> {
    args(&["-c:a", "libmp3lame", "-b:a", "160k", "-f", "mp3", "pipe:1"])
}

/// Raw AAC in ADTS frames: each frame carries its header, so listeners can join at any moment.
pub fn aac_args() -> Vec<String> {
    args(&["-c:a", "aac", "-b:a", "128k", "-f", "adts", "pipe:1"])
}

pub fn hls_args(dir: &Path) -> Vec<String> {
    let playlist = dir.join("index.m3u8").to_string_lossy().into_owned();
    args(&["-c:a", "aac", "-b:a", "128k", "-f", "hls", "-hls_time", "4", "-hls_list_size", "6", "-hls_flags", "delete_segments", &playlist])
}

/// One ffmpeg fed from the PCM bus, restarted after a pause when it exits. A slow output lags only its own receiver.
/// With `restart`, a bump starts it again with fresh arguments (the relay on a new path).
pub async fn encoder(
    st: Arc<Station>,
    name: &'static str,
    args: impl Fn() -> Vec<String> + Send + 'static,
    out: Option<broadcast::Sender<Bytes>>,
    restart: bool,
) {
    let mut closed = st.closed.subscribe();
    let mut moved = st.moved.subscribe();
    while !st.is_closed() {
        let child = Command::new("ffmpeg")
            .args(args())
            .stdin(Stdio::piped())
            .stdout(if out.is_some() { Stdio::piped() } else { Stdio::null() })
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn();
        let Ok(mut child) = child else {
            tracing::warn!(station = %st.id, encoder = name, "cannot start ffmpeg");
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        };
        let mut stdin = child.stdin.take().expect("piped stdin");
        if let (Some(tx), Some(mut stdout)) = (out.clone(), child.stdout.take()) {
            tokio::spawn(async move {
                let mut b = vec![0u8; 4096];
                while let Ok(n) = stdout.read(&mut b).await {
                    if n == 0 {
                        break;
                    }
                    let _ = tx.send(Bytes::copy_from_slice(&b[..n]));
                }
            });
        }
        let mut pcm = st.pcm.subscribe();
        loop {
            tokio::select! {
                chunk = pcm.recv() => match chunk {
                    Ok(c) => if stdin.write_all(&c).await.is_err() { break },
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => return,
                },
                _ = closed.changed() => { let _ = child.kill().await; return; }
                _ = moved.changed(), if restart => { let _ = child.kill().await; break; }
            }
        }
        if !st.is_closed() {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
}

/// Counts one listener until dropped.
pub struct ListenerGuard(Arc<AtomicUsize>);

impl ListenerGuard {
    pub fn new(count: Arc<AtomicUsize>) -> Self {
        count.fetch_add(1, Ordering::Relaxed);
        Self(count)
    }
}

impl Drop for ListenerGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}
