---
status: accepted
date: 2026-10-19
decision-makers: fulgas
---

# Leptos SSR; the web app uses only the public API

## Context and Problem Statement

A web app with private endpoints grows behaviour scripts cannot reach, and two ways to do one thing drift apart. The web app should be fast to first paint and keyboard-friendly without a separate JavaScript toolchain. How is the web app built, and how does it reach the server?

## Considered Options

- Leptos SSR with hydration, calling the public JSON API
- Leptos server functions
- A separate single-page app in TypeScript

## Decision Outcome

Chosen option: "Leptos SSR with hydration, calling the public JSON API", because it keeps one way to do everything: the web app renders on the server, hydrates, and calls `/api/v1` like any script. thaw supplies the look and toasts; the `ui` module uses only `domain` types.

### Consequences

- Good, because scripts with the API token can do anything the web app can.
- Good, because the end-to-end tests exercise the same API scripts use.
- Bad, because some screens make more calls than server functions would.

### Confirmation

`tests/layers.rs` keeps `ui` off the server modules; the e2e suite drives every screen through the API.
