//! Maio, one crate in four layers (SPEC 3). Each layer may use only the ones above it:
//! `domain` (rules, no I/O) ← `usecases` (use cases and ports) ← `adapters` (SQLite, engines, ffmpeg) ← `web` (API, config, telemetry).
//! `tests/layers.rs` keeps it that way.

pub mod adapters;
pub mod domain;
pub mod usecases;
pub mod web;
