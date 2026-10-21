//! The engine panel's controls (arc42 3.5, 8.5).

use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;

use super::Body;
use crate::web::{auth::Me, error::ApiError, state::Shared};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Switch {
    model: String,
}

pub async fn switch_model(State(app): State<Shared>, _me: Me, Path(id): Path<String>, Body(b): Body<Switch>) -> Result<StatusCode, ApiError> {
    app.making.switch_model(&id, &b.model).await?;
    tracing::info!(engine = %id, model = %b.model, "engine model switched");
    Ok(StatusCode::NO_CONTENT)
}
