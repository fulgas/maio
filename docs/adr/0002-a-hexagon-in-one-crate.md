---
status: accepted
date: 2026-10-19
decision-makers: fulgas
---

# A hexagon in one crate, its direction tested

## Context and Problem Statement

The rules (what a song is, when a playlist may be deleted, how titles are chosen) must stay testable without HTTP, SQLite, ffmpeg or a browser, and adapters change more often than rules. v1 first had a workspace of crates per layer; the boundaries cost build time and ceremony for one binary. How are the layers kept apart?

## Considered Options

- One crate, a module per layer, the direction checked by a test
- A crate per layer
- No layering

## Decision Outcome

Chosen option: "One crate, a module per layer, the direction checked by a test", because it keeps the rules free of infrastructure without a crate per layer: `domain` ← `usecases` ← `adapters` ← `web`, plus `ui`, which uses only `domain`. Use cases own small port traits; adapters implement them; tests use in-memory fakes.

### Consequences

- Good, because rules are unit-tested in microseconds and an adapter can be swapped without touching a use case.
- Bad, because the direction is held by a test rather than the compiler.

### Confirmation

`tests/layers.rs` reads the sources and fails the build when a layer uses one below it; it runs in `just test`.
