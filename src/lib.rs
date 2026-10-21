//! Maio, one crate in five layers (arc42 5). Each layer may use only the ones above it:
//! `domain` (rules, no I/O) ← `usecases` (use cases and ports) ← `adapters` (SQLite, engines, ffmpeg) ← `web` (API, config, telemetry).
//! `ui` is the web app; it runs in the browser too, so it uses only `domain` and talks to the server through the API.
//! `tests/layers.rs` keeps it that way.

// Leptos view types nest deeply; the Songs screen needs more than the default 128.
#![recursion_limit = "256"]

pub mod domain;
pub mod ui;

#[cfg(feature = "ssr")]
pub mod adapters;
#[cfg(feature = "ssr")]
pub mod usecases;
#[cfg(feature = "ssr")]
pub mod web;

/// The browser's entry point: hydrates the server-rendered page.
#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(ui::App);
}
