//! Songs and the jobs that make them (SPEC 5, 10).

use crate::domain::{job::Job, request::SongRequest};
use crate::usecases::{Accepted, SongPage, SongView, ports::SongSort};
use axum::{
    Json,
    extract::{Path, Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use tower::ServiceExt;
use tower_http::services::ServeFile;

use super::{Body, Params, blocking};
use crate::web::{auth::Me, error::ApiError, state::Shared};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListQuery {
    q: Option<String>,
    sort: Option<String>,
    limit: Option<usize>,
    cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rename {
    title: String,
}

pub async fn make(State(app): State<Shared>, _me: Me, Body(request): Body<SongRequest>) -> Result<(StatusCode, Json<Accepted>), ApiError> {
    Ok((StatusCode::ACCEPTED, Json(app.making.make(&request)?)))
}

pub async fn jobs(State(app): State<Shared>, _me: Me) -> Json<Vec<Job>> {
    Json(app.making.jobs())
}

pub async fn cancel(State(app): State<Shared>, _me: Me, Path(id): Path<String>) -> Result<Json<Job>, ApiError> {
    Ok(Json(app.making.cancel(&id)?))
}

pub async fn list(State(app): State<Shared>, _me: Me, Params(query): Params<ListQuery>) -> Result<Json<SongPage>, ApiError> {
    let sort = match query.sort.as_deref() {
        None | Some("newest") => SongSort::Newest,
        Some("title") => SongSort::Title,
        Some(_) => return Err(ApiError::invalid("bad_sort", "sort is newest or title")),
    };
    let page = blocking(&app, move |a| a.songs.search(query.q.as_deref(), sort, query.limit, query.cursor.as_deref())).await?;
    Ok(Json(page))
}

pub async fn get(State(app): State<Shared>, _me: Me, Path(id): Path<String>) -> Result<Json<SongView>, ApiError> {
    Ok(Json(blocking(&app, move |a| a.songs.get(&id)).await?))
}

pub async fn rename(State(app): State<Shared>, _me: Me, Path(id): Path<String>, Body(b): Body<Rename>) -> Result<Json<SongView>, ApiError> {
    let view = blocking(&app, move |a| a.songs.rename(&id, &b.title)).await?;
    app.events.song_changed(&view.song.id, false);
    Ok(Json(view))
}

pub async fn delete(State(app): State<Shared>, _me: Me, Path(id): Path<String>) -> Result<StatusCode, ApiError> {
    let gone = id.clone();
    blocking(&app, move |a| a.songs.delete(&id)).await?;
    tracing::info!(song = %gone, "song deleted");
    app.events.song_changed(&gone, true);
    Ok(StatusCode::NO_CONTENT)
}

/// The song's file, with range requests so the player can seek (SPEC 8).
pub async fn audio(State(app): State<Shared>, _me: Me, Path(id): Path<String>, req: Request) -> Result<Response, ApiError> {
    let view = blocking(&app, move |a| a.songs.get(&id)).await?;
    match ServeFile::new(app.data_dir.join(&view.song.file)).oneshot(req).await {
        Ok(res) => Ok(res.into_response()),
        Err(never) => match never {},
    }
}
