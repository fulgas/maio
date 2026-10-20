//! Maio's web layer (arc42 5, 3.5, 8.5): config, the JSON API, events, and the web app's pages over axum.

pub mod api;
pub mod auth;
pub mod config;
pub mod error;
pub mod events;
mod icy;
pub mod live;
pub mod metrics;
pub mod pages;
mod start;
pub mod state;
pub mod streams;
pub mod telemetry;

pub use api::router;
pub use start::start;
pub use state::{App, Shared};
