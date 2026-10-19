//! Sample values for tests in this crate and the ones above it.

use crate::domain::{Commercial, Licence, Song, TitleSource};

/// A finished instrumental song with plausible fields.
pub fn song(id: &str) -> Song {
    Song {
        id: id.into(),
        title: format!("Song {id}"),
        title_source: TitleSource::Words,
        description: None,
        prompt: "warm pads".into(),
        mood: Some("calm".into()),
        genre: Some("ambient".into()),
        energy: None,
        seconds: 180.0,
        bpm: Some(70),
        seed: 7,
        vocals: false,
        key: None,
        time_signature: None,
        lyrics: None,
        vocal_language: None,
        engine: "ace-step".into(),
        licence: Licence { name: "MIT".into(), commercial: Commercial::Yes, note: String::new() },
        lyrics_engine: None,
        title_engine: None,
        artist: "Maio".into(),
        file: format!("songs/{id}.opus"),
        bytes: 1000,
        created_at: "2026-10-16T12:00:00Z".into(),
    }
}

use crate::domain::{
    engine::{Caps, EngineInfo},
    request::Normalized,
};

/// A checked request for a calm instrumental of 180 s.
pub fn normalized() -> Normalized {
    Normalized {
        engine_prompt: "calm mood, instrumental".into(),
        prompt: None,
        mood: Some("calm".into()),
        genre: None,
        energy: None,
        seconds: 180,
        bpm: None,
        vocals: false,
        lyrics: None,
        vocal_language: "en".into(),
        lyrics_by: "ace-step".into(),
        engine: None,
        seed: 7,
        allow_noncommercial: false,
    }
}

/// An engine that makes 10 to 600 s, with vocals, one job at a time, at half real time.
pub fn engine_info(id: &str, commercial: Commercial) -> EngineInfo {
    EngineInfo {
        id: id.into(),
        name: id.into(),
        version: "1".into(),
        licence: Licence { name: "MIT".into(), commercial, note: String::new() },
        capabilities: Caps { min_seconds: 10, max_seconds: 600, vocals: true, tempo: true, concurrency: 1 },
        realtime_factor: 0.5,
        model: "acestep-v15-turbo".into(),
        lm_model: None,
        models: vec![],
        queued: 0,
        running: 0,
    }
}
