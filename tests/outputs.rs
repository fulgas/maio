mod common;

use std::time::Duration;

use bytes::Bytes;
use common::test_state;
use maio::{outputs, playout::CHUNK};

/// Feeds silence into the PCM bus the way playout does, for `secs`.
fn feed(state: &maio::state::AppState, secs: u64) {
    let pcm = state.main().pcm.clone();
    tokio::spawn(async move {
        for _ in 0..secs * 50 {
            let _ = pcm.send(Bytes::from(vec![0u8; CHUNK * 2]));
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    });
}

fn output_state(state: &maio::state::AppState, name: &str) -> &'static str {
    state.main().outputs.lock().unwrap().iter().find(|o| o.name == name).unwrap().state
}

#[tokio::test]
async fn mp3_encoder_produces_bytes_for_listeners() {
    let (state, _dir) = test_state(vec![], "").await;
    let mut listener = state.main().mp3.subscribe();
    outputs::spawn_encoder(&state, &state.main(), "mp3".into(), "pull".into(), "/stream.mp3".into(), outputs::mp3_args(), Some(state.main().mp3.clone()));
    feed(&state, 4);
    let got = tokio::time::timeout(Duration::from_secs(5), listener.recv()).await.expect("mp3 bytes within 5 s").unwrap();
    assert!(!got.is_empty());
}

#[tokio::test]
async fn unreachable_push_target_goes_down_without_blocking_others() {
    let (state, _dir) = test_state(vec![], "").await;
    let bad = maio::config::OutputConfig {
        kind: maio::config::OutputKind::Rtsp,
        codec: maio::config::Codec::Opus,
        url: "rtsp://127.0.0.1:1/music".into(),
        bitrate_k: 96,
        radio: "main".into(),
    };
    outputs::spawn_encoder(&state, &state.main(), "rtsp-0".into(), "rtsp".into(), bad.url.clone(), outputs::push_args(&bad), None);
    let mut listener = state.main().mp3.subscribe();
    outputs::spawn_encoder(&state, &state.main(), "mp3".into(), "pull".into(), "/stream.mp3".into(), outputs::mp3_args(), Some(state.main().mp3.clone()));
    feed(&state, 6);
    let mut down = false;
    for _ in 0..50 {
        if output_state(&state, "rtsp-0") == "down" {
            down = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(down, "rtsp output should report down");
    let got = tokio::time::timeout(Duration::from_secs(5), listener.recv()).await;
    assert!(got.is_ok(), "mp3 listeners keep receiving");
}

#[tokio::test]
async fn aac_encoder_produces_adts_frames_for_listeners() {
    let (state, _dir) = test_state(vec![], "").await;
    let mut listener = state.main().aac.subscribe();
    outputs::spawn_encoder(&state, &state.main(), "aac".into(), "pull".into(), "/stream.aac".into(), outputs::aac_args(), Some(state.main().aac.clone()));
    feed(&state, 4);
    let got = tokio::time::timeout(Duration::from_secs(5), listener.recv()).await.expect("aac bytes within 5 s").unwrap();
    assert!(got.len() >= 2 && got[0] == 0xFF && got[1] & 0xF0 == 0xF0, "starts with an ADTS sync word");
}
