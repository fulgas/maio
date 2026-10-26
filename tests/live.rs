//! Live stations with real ffmpeg (arc42 8.4): songs decode, crossfade and stream as MP3.

use std::{process::Command, sync::Arc, time::Duration};

use maio::{
    adapters::live::{Stations, StationsConfig},
    domain::{Live, Playlist, Visibility, live::StreamFormat as Format},
    usecases::ports::{Change, LiveListening, LiveSong, LiveStreams},
};

/// Writes `songs/{id}.opus`: a `seconds`-long sine tone at `hz`.
fn tone(dir: &std::path::Path, id: &str, hz: u32, seconds: u32) -> LiveSong {
    std::fs::create_dir_all(dir.join("songs")).unwrap();
    let file = format!("songs/{id}.opus");
    let ok = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", &format!("sine=frequency={hz}:duration={seconds}"), "-ac", "2", "-c:a", "libopus"])
        .arg(dir.join(&file))
        .status()
        .unwrap()
        .success();
    assert!(ok, "ffmpeg made the tone");
    LiveSong { id: id.into(), title: format!("Tone {id}"), file, seconds: f64::from(seconds) }
}

fn playlist(visibility: Visibility) -> Playlist {
    let mut p = Playlist::new("p1".into(), "Late", "k".repeat(32), "t".into()).unwrap();
    p.live = Live { on: true, shuffle: false, visibility, key: "k".repeat(32) };
    p
}

fn stations(dir: &std::path::Path) -> Arc<Stations> {
    telling(dir).0
}

/// Stations, and the changes they send to the event queue.
fn telling(dir: &std::path::Path) -> (Arc<Stations>, tokio::sync::mpsc::UnboundedReceiver<Change>) {
    let (changes, told) = tokio::sync::mpsc::unbounded_channel();
    (Stations::new(StationsConfig { data_dir: dir.into(), crossfade_s: 0.5, mp3: true, aac: true, hls: false, relay: None }, changes), told)
}

async fn first_bytes(mut rx: tokio::sync::broadcast::Receiver<bytes::Bytes>, n: usize) -> Vec<u8> {
    let mut got = vec![];
    while got.len() < n {
        match tokio::time::timeout(Duration::from_secs(10), rx.recv()).await {
            Ok(Ok(b)) => got.extend_from_slice(&b),
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => {}
            other => panic!("no stream bytes: {other:?}"),
        }
    }
    got
}

#[tokio::test(flavor = "multi_thread")]
async fn a_live_playlist_streams_mp3_and_moves_through_its_songs() {
    let dir = tempfile::tempdir().unwrap();
    let songs = vec![tone(dir.path(), "s1", 440, 2), tone(dir.path(), "s2", 660, 2)];
    let st = stations(dir.path());
    st.play(&playlist(Visibility::Public), &songs);
    let listen = st.listen("p1", Format::Mp3).expect("on air");
    let bytes = first_bytes(listen.bytes, 4096).await;
    let sync = bytes.windows(2).any(|w| w[0] == 0xFF && w[1] & 0xE0 == 0xE0);
    assert!(sync, "MP3 frame sync in the stream");
    assert_eq!(st.on_air("p1").unwrap().now.unwrap().id, "s1");
    // The 2 s song ends and the next starts; a slow machine takes longer, so wait for it rather than a fixed time.
    let next = tokio::time::timeout(Duration::from_secs(15), async {
        while st.on_air("p1").and_then(|o| o.now).is_none_or(|s| s.id != "s2") {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });
    assert!(next.await.is_ok(), "s2 came on air after s1");
    assert_eq!(st.now_title("p1").as_deref(), Some("Tone s2"));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_next_song_shows_as_soon_as_a_song_starts() {
    let dir = tempfile::tempdir().unwrap();
    let songs = vec![tone(dir.path(), "s1", 440, 30), tone(dir.path(), "s2", 660, 30)];
    let st = stations(dir.path());
    st.play(&playlist(Visibility::Public), &songs);
    let _ = first_bytes(st.listen("p1", Format::Mp3).expect("on air").bytes, 4096).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let on = st.on_air("p1").unwrap();
    assert_eq!(on.now.unwrap().id, "s1");
    assert_eq!(on.next.map(|n| n.id).as_deref(), Some("s2"), "next is known for the whole song, not only its last seconds");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_song_on_air_its_next_song_and_each_listener_are_told() {
    let dir = tempfile::tempdir().unwrap();
    let songs = vec![tone(dir.path(), "s1", 440, 30), tone(dir.path(), "s2", 660, 30)];
    let (st, mut told) = telling(dir.path());
    st.play(&playlist(Visibility::Public), &songs);
    let listen = st.listen("p1", Format::Mp3).expect("on air");
    let _ = first_bytes(listen.bytes, 4096).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let changes: Vec<Change> = std::iter::from_fn(|| told.try_recv().ok()).collect();
    assert!(changes.iter().all(|c| *c == Change::Live("p1".into())), "{changes:?}");
    // s1 picked as next, s1 on air, s2 picked as next, one listener: the web app sees the next song too.
    assert!(changes.len() >= 3, "{changes:?}");
    drop(listen.guard);
    assert_eq!(told.try_recv().ok(), Some(Change::Live("p1".into())), "a listener leaving is told");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_empty_live_playlist_streams_silence() {
    let dir = tempfile::tempdir().unwrap();
    let st = stations(dir.path());
    st.play(&playlist(Visibility::Public), &[]);
    let listen = st.listen("p1", Format::Aac).expect("on air");
    let bytes = first_bytes(listen.bytes, 1024).await;
    assert!(bytes.windows(2).any(|w| w[0] == 0xFF && w[1] & 0xF0 == 0xF0), "ADTS frames");
    assert_eq!(st.on_air("p1").unwrap().now, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_song_that_cannot_be_decoded_is_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let broken = LiveSong { id: "gone".into(), title: "Gone".into(), file: "songs/gone.opus".into(), seconds: 3.0 };
    let songs = vec![broken, tone(dir.path(), "s1", 440, 3)];
    let st = stations(dir.path());
    st.play(&playlist(Visibility::Public), &songs);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(st.on_air("p1").unwrap().now.map(|s| s.id).as_deref(), Some("s1"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stalled_listener_does_not_stall_the_stream() {
    let dir = tempfile::tempdir().unwrap();
    let st = stations(dir.path());
    st.play(&playlist(Visibility::Public), &[tone(dir.path(), "s1", 440, 4)]);
    let _stalled = st.listen("p1", Format::Mp3).unwrap(); // never read
    let listen = st.listen("p1", Format::Mp3).unwrap();
    let bytes = first_bytes(listen.bytes, 16_384).await;
    assert!(bytes.len() >= 16_384, "the reading listener keeps getting bytes");
}

#[tokio::test(flavor = "multi_thread")]
async fn off_air_ends_the_streams_and_a_new_address_moves_listeners() {
    let dir = tempfile::tempdir().unwrap();
    let st = stations(dir.path());
    let mut p = playlist(Visibility::Public);
    st.play(&p, &[tone(dir.path(), "s1", 440, 4)]);
    let mut listen = st.listen("p1", Format::Mp3).unwrap();
    p.live.visibility = Visibility::Private;
    st.play(&p, &[tone(dir.path(), "s1", 440, 4)]);
    tokio::time::timeout(Duration::from_secs(2), listen.moved.changed()).await.expect("moved").unwrap();
    assert_eq!(st.on_air("p1").unwrap().listeners, 1, "the guard still counts until the stream ends");
    drop(listen);
    st.stop("p1");
    assert!(st.listen("p1", Format::Mp3).is_none());
    assert!(st.on_air("p1").is_none());
}
