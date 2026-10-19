//! `GET /status` (SPEC 10): engines with health, live playlists, disk usage, moods and genres.

use axum::{Json, extract::State};
use serde_json::{Value, json};

use super::blocking;
use crate::web::{auth::Me, error::ApiError, state::Shared};

pub async fn status(State(app): State<Shared>, _me: Me) -> Result<Json<Value>, ApiError> {
    let (songs_bytes, live) = blocking(&app, |a| {
        let live: Vec<Value> = a.playlists.list()?.into_iter().filter(|p| p.live.on).map(|p| json!({"id": p.id, "name": p.name})).collect();
        Ok((a.songs.total_bytes()?, live))
    })
    .await?;
    Ok(Json(json!({
        "engines": app.making.engines(),
        "text_engines": app.making.text_engines(),
        "live": live,
        "disk": {"songs_bytes": songs_bytes},
        "moods": app.moods,
        "genres": app.genres,
    })))
}
