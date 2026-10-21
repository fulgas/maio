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
        state::Shared,
    },
};

async fn view(app: &Shared, id: String) -> Result<Json<LiveJson>, ApiError> {
    let v = blocking(app, move |a| a.playlists.live(&id)).await?;
    Ok(Json(json(v, &app.serve, app.relay.as_ref())))
}

pub async fn get(State(app): State<Shared>, _me: Me, Path(id): Path<String>) -> Result<Json<LiveJson>, ApiError> {
    view(&app, id).await
}

pub async fn set(State(app): State<Shared>, _me: Me, Path(id): Path<String>, Body(change): Body<LiveChange>) -> Result<Json<LiveJson>, ApiError> {
    let pid = id.clone();
    blocking(&app, move |a| a.playlists.set_live(&pid, &change)).await?;
    app.events.live_changed(&id);
    view(&app, id).await
}

pub async fn new_key(State(app): State<Shared>, _me: Me, Path(id): Path<String>) -> Result<Json<LiveJson>, ApiError> {
    let pid = id.clone();
    blocking(&app, move |a| a.playlists.new_key(&pid)).await?;
    app.events.live_changed(&id);
    view(&app, id).await
}
