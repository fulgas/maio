//! The live switch over the API (arc42 3.5): `PUT`/`GET /playlists/{id}/live`, `POST /playlists/{id}/live/key`.

use axum::{
    Json,
    extract::{Path, State},
};

use super::{Body, blocking};
use crate::{
    domain::LiveChange,
    web::{
        auth::Me,
        error::ApiError,
        live::{LiveJson, json},
        push::{self, Change},
        state::Shared,
    },
};

async fn view(app: &Shared, id: String) -> Result<Json<LiveJson>, ApiError> {
    let v = blocking(app, move |a| a.playlists.live(&id)).await?;
    let relay = app.live.relay_addresses(&v.playlist);
    Ok(Json(json(v, &app.serve, relay)))
}

pub async fn get(State(app): State<Shared>, _me: Me, Path(id): Path<String>) -> Result<Json<LiveJson>, ApiError> {
    view(&app, id).await
}

pub async fn set(State(app): State<Shared>, _me: Me, Path(id): Path<String>, Body(change): Body<LiveChange>) -> Result<Json<LiveJson>, ApiError> {
    let pid = id.clone();
    blocking(&app, move |a| a.playlists.set_live(&pid, &change)).await?;
    // Every live strip shows how many streams are in use, so the other live playlists are told too.
    let others = blocking(&app, |a| a.playlists.all_raw()).await?.into_iter().filter(|p| p.live.on && p.id != id).map(|p| Change::Live(p.id));
    let changes = [Change::Live(id.clone()), Change::Playlist(id.clone())].into_iter().chain(others).collect();
    push::send(&app, changes).await;
    view(&app, id).await
}

pub async fn new_key(State(app): State<Shared>, _me: Me, Path(id): Path<String>) -> Result<Json<LiveJson>, ApiError> {
    let pid = id.clone();
    blocking(&app, move |a| a.playlists.new_key(&pid)).await?;
    push::send(&app, vec![Change::Live(id.clone())]).await;
    view(&app, id).await
}
