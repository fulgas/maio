//! Maio's web layer (SPEC 3, 10): config, the JSON API and events over axum. The Leptos app joins in plan 5.

pub mod api;
pub mod auth;
pub mod config;
pub mod error;
pub mod events;
pub mod metrics;
mod start;
pub mod state;
pub mod telemetry;

pub use api::router;
pub use start::start;
pub use state::{App, Shared};
