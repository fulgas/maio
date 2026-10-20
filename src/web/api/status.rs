//! `GET /status` (arc42 3.5): engines with health, live playlists, MediaMTX, disk usage, moods and genres.

use axum::{Json, extract::State};
use serde_json::{Value, json};

use super::blocking;
use crate::web::{auth::Me, error::ApiError, state::Shared};

pub async fn status(State(app): State<Shared>, _me: Me) -> Result<Json<Value>, ApiError> {
    let (songs_bytes, live, relay) = blocking(&app, |a| {
        let live: Vec<_> = a.playlists.all_raw()?.into_iter().filter(|p| p.live.on).collect();
        Ok((a.songs.total_bytes()?, live, a.live.relay_status()?))
    })
    .await?;
    let live: Vec<Value> = live.iter().map(|p| json!({"id": p.id, "name": p.name})).collect();
    Ok(Json(json!({
        "engines": app.making.engines(),
        "text_engines": app.making.text_engines(),
        "live": live,
        "relay": relay,
        "disk": {"songs_bytes": songs_bytes},
        "moods": app.moods,
        "genres": app.genres,
    })))
}
