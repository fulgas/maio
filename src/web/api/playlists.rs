//! Playlists (arc42 8.3, 3.5). Their live settings come in plan 4.

use crate::domain::Playlist;
use crate::usecases::{PlaylistSummary, PlaylistView};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;

use super::{Body, blocking};
use crate::web::push::{self, Change};
use crate::web::{
    auth::Me,
    error::ApiError,
    state::{App, Shared},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Named {
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Add {
    song_ids: Vec<String>,
    #[serde(default)]
    position: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Order {
    song_ids: Vec<String>,
}

/// Runs a change and tells every browser about it.
async fn change(app: &Shared, f: impl FnOnce(&App) -> crate::usecases::Result<Playlist> + Send + 'static) -> Result<Json<Playlist>, ApiError> {
    let playlist = blocking(app, f).await?;
    push::send(app, vec![Change::Playlist(playlist.id.clone())]).await;
    Ok(Json(playlist))
}

pub async fn list(State(app): State<Shared>, _me: Me) -> Result<Json<Vec<PlaylistSummary>>, ApiError> {
    Ok(Json(blocking(&app, |a| a.playlists.list()).await?))
}

pub async fn create(State(app): State<Shared>, _me: Me, Body(b): Body<Named>) -> Result<(StatusCode, Json<Playlist>), ApiError> {
    Ok((StatusCode::CREATED, change(&app, move |a| a.playlists.create(&b.name)).await?))
}

pub async fn get(State(app): State<Shared>, _me: Me, Path(id): Path<String>) -> Result<Json<PlaylistView>, ApiError> {
    Ok(Json(blocking(&app, move |a| a.playlists.get(&id)).await?))
}

pub async fn rename(State(app): State<Shared>, _me: Me, Path(id): Path<String>, Body(b): Body<Named>) -> Result<Json<Playlist>, ApiError> {
    change(&app, move |a| a.playlists.rename(&id, &b.name)).await
}

pub async fn delete(State(app): State<Shared>, _me: Me, Path(id): Path<String>) -> Result<StatusCode, ApiError> {
    let gone = id.clone();
    // Its songs lose its mark: they are told after it is gone.
    let songs = blocking(&app, move |a| {
        let songs = a.playlists.get(&id)?.songs;
        a.playlists.delete(&id)?;
        Ok(songs)
    })
    .await?;
    tracing::info!(playlist = %gone, "playlist deleted");
    let changes = std::iter::once(Change::Playlist(gone)).chain(songs.into_iter().map(|s| Change::Song(s.id)));
    push::send(&app, changes.collect()).await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn add(State(app): State<Shared>, _me: Me, Path(id): Path<String>, Body(b): Body<Add>) -> Result<Json<Playlist>, ApiError> {
    change(&app, move |a| a.playlists.add_songs(&id, &b.song_ids, b.position)).await
}

pub async fn reorder(State(app): State<Shared>, _me: Me, Path(id): Path<String>, Body(b): Body<Order>) -> Result<Json<Playlist>, ApiError> {
    change(&app, move |a| a.playlists.reorder(&id, b.song_ids)).await
}

pub async fn remove(State(app): State<Shared>, _me: Me, Path((id, song_id)): Path<(String, String)>) -> Result<Json<Playlist>, ApiError> {
    let removed = song_id.clone();
    let playlist = change(&app, move |a| a.playlists.remove_song(&id, &song_id)).await?;
    push::send(&app, vec![Change::Song(removed)]).await; // no longer in the playlist, so not told with it
    Ok(playlist)
}
