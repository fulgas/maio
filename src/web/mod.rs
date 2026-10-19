//! Maio's web layer (SPEC 3, 10, 12): config, the JSON API, events, and the web app's pages over axum.

pub mod api;
pub mod auth;
pub mod config;
pub mod error;
pub mod events;
pub mod metrics;
pub mod pages;
mod start;
pub mod state;
pub mod telemetry;

pub use api::router;
pub use start::start;
pub use state::{App, Shared};
