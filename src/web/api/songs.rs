//! Songs and the jobs that make them (arc42 8.2, 3.5).

use crate::domain::{job::Job, request::SongRequest};
use crate::usecases::{Accepted, SongPage, SongView, ports::SongSort};
use axum::{
    Json,
    extract::{Path, Request, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use tower::ServiceExt;
use tower_http::services::ServeFile;

use super::{Body, Params, blocking};
use crate::web::push::Change;
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
    crate::web::push::send(&app, vec![Change::Song(view.song.id.clone())]).await;
    Ok(Json(view))
}

pub async fn delete(State(app): State<Shared>, _me: Me, Path(id): Path<String>) -> Result<StatusCode, ApiError> {
    let gone = id.clone();
    blocking(&app, move |a| a.songs.delete(&id)).await?;
    tracing::info!(song = %gone, "song deleted");
    crate::web::push::send(&app, vec![Change::Song(gone)]).await;
    Ok(StatusCode::NO_CONTENT)
}

/// The song's file, with range requests so the player can seek (arc42 8.3).
pub async fn audio(State(app): State<Shared>, _me: Me, Path(id): Path<String>, req: Request) -> Result<Response, ApiError> {
    let view = blocking(&app, move |a| a.songs.get(&id)).await?;
    match ServeFile::new(app.data_dir.join(&view.song.file)).oneshot(req).await {
        Ok(res) => Ok(res.into_response()),
        Err(never) => match never {},
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DownloadQuery {
    format: String,
}

/// The song in a format, as an attachment named after its title (arc42 3.5). A converted copy is removed as soon as
/// it is open: the open file is sent in full while its name is already gone (Unix).
pub async fn download(
    State(app): State<Shared>,
    _me: Me,
    Path(id): Path<String>,
    Params(q): Params<DownloadQuery>,
    req: Request,
) -> Result<Response, ApiError> {
    let d = blocking(&app, move |a| a.songs.download(&id, &q.format)).await?;
    let res = match ServeFile::new(&d.path).oneshot(req).await {
        Ok(res) => res,
        Err(never) => match never {},
    };
    if d.temporary {
        let _ = std::fs::remove_file(&d.path);
    }
    if res.status() == axum::http::StatusCode::NOT_FOUND {
        // The file went between export and sending: an API error, never an empty "attachment".
        return Err(ApiError::not_found("the song's file is missing"));
    }
    let mut res = res.into_response();
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(d.format.content_type()));
    if let Ok(v) = HeaderValue::from_str(&content_disposition(&d.name)) {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    Ok(res)
}

/// `attachment` with an ASCII name for old clients and the real one, percent-encoded, for the rest (RFC 6266).
fn content_disposition(name: &str) -> String {
    let ascii: String = name.chars().map(|c| if c.is_ascii() && !c.is_ascii_control() && c != '"' && c != '\\' { c } else { '_' }).collect();
    let encoded: String =
        name.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect();
    format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{encoded}")
}

#[cfg(test)]
mod tests {
    use super::content_disposition;

    #[test]
    fn content_disposition_keeps_unicode() {
        assert_eq!(content_disposition("東京.mp3"), "attachment; filename=\"__.mp3\"; filename*=UTF-8''%E6%9D%B1%E4%BA%AC.mp3");
        assert_eq!(content_disposition("a\"b.wav"), "attachment; filename=\"a_b.wav\"; filename*=UTF-8''a%22b.wav");
    }
}
