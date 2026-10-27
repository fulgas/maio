# 5. Building block view

Maio is written as a hexagon: a core that knows nothing about HTTP, SQLite, ffmpeg or Leptos, surrounded by adapters that plug into ports the core defines ([ADR 0002](../adr/0002-a-hexagon-in-one-crate.md)).

## What hexagonal means

Hexagonal architecture (also called ports and adapters) keeps what Maio *does* apart from what it *uses*. The six sides of the hexagon mean nothing; the shape only says the core has many sides where things can plug in.

- **The core** is `domain` (the rules: what a song is, when a playlist may be deleted, how titles are chosen) and `usecases` (one per thing a person does: make a song, go live, sign in). It has no HTTP, SQL, files, processes or browsers in it.
- **Ports** are the core's own traits, in `src/usecases/ports.rs`: small, one role each, written in the core's words (`SongRepository::insert`, not "run this SQL"). The core decides what it needs; the outside world fits in.
- **Driving adapters** call into the core: `web` (the JSON API, the streams, the pages) and, through the API, `ui`. A request becomes a use case call; the answer becomes JSON.
- **Driven adapters** are called by the core through its ports: SQLite, ACE-Step and the text engines over HTTP, ffmpeg, the live stations and MediaMTX, passwords, the clock.
- **Dependencies point inward.** `domain` uses nothing; `usecases` use only `domain`; adapters use the core, never the other way. `tests/layers.rs` fails the build when an arrow points outward, and within `web` only the start-up wiring (`start.rs`, `config.rs`) may name an adapter.

What it buys: every rule is tested in microseconds with in-memory fakes in place of the adapters (5.2); an adapter (another engine, another store) is replaced without touching a rule; and a request handler stays a translation, with no business rule in it.

The C4 model draws these blocks and how they talk:

```c4
components
```

## 5.1 Level 1: the layers

One crate, `maio`, with one module per layer. A layer uses only the layers above it; `tests/layers.rs` fails the build otherwise.

```tree
src/
  domain/      types and rules; no dependencies beyond serde
  usecases/    use cases and port traits; uses domain
  adapters/    sqlite, engines, text engines, ffmpeg, live, relay; implement the ports in usecases
  web/         the axum server: JSON API, streams, pages (Leptos SSR), config, telemetry; wires everything
  ui/          the web app (Leptos components); uses only domain, talks to the server through the API
  main.rs      the maio binary
tests/         tests through the public API of each layer
e2e/           a real browser through every screen, against a fake acestep-api and a fake MediaMTX control API
```

| Layer | Responsibility |
|-------|----------------|
| **Domain** | Plain types and rules: `Song`, `Playlist`, `Live` state, song requests, titles in layers, engine choice, jobs, the live play order and limits, MediaMTX's reported state. No I/O, no async, no framework types. Every rule is tested here with plain unit tests. |
| **Use cases** | One use case per user action (make a song, cancel a job, rename, delete, create a playlist, add, remove, reorder, go live, change visibility, new link, listen live, answer MediaMTX's auth hook, sign in, change password). Each takes its ports as traits and returns domain results or domain errors. |
| **Adapters** | Each behind one port: SQLite, the engine HTTP client, the text engine clients, ffmpeg, the live stations and the MediaMTX relay, passwords, clock and ids. |
| **Web** | axum: the JSON API (arc42 3.5), the server-sent events, each built from the state as it is now (`web/push.rs`), the live streams with ICY titles (`web/icy.rs`) and the relay auth hook (arc42 8.4), server-rendered pages, config (arc42 7.3), logs, traces and metrics (arc42 8.6). It maps domain errors to HTTP status codes. No business rule lives in a handler. |
| **UI** | The web app (arc42 8.5): Leptos components rendered on the server and hydrated in the browser. It uses only `domain` types and calls `/api/v1` like any script. No business rule lives in a component. |

## 5.2 Level 2: the ports

Traits owned by the use cases, each small and role-specific; tests swap every one for an in-memory fake (`usecases::testing`).

| Port | Does | Adapter | Fake in tests |
|------|------|---------|---------------|
| `SongRepository`, `PlaylistRepository` | store and find songs and playlists | `adapters::sqlite` | `MemSongs`, `MemPlaylists` |
| `AccountStore` | the one account and its sessions | `adapters::sqlite` | `MemAccount` |
| `Passwords` | hash and check passwords (Argon2id) | `adapters::passwords` | `PlainPasswords` |
| `MusicEngine` | health, submit, poll, download a generation, switch model (arc42 3.3) | `adapters::engine` | `FakeEngine` |
| `TextEngine` | write lyrics, name a song, health (arc42 3.4) | `adapters::text` | `FakeText` |
| `AudioFiles` | import (transcode, normalise), tag, duration, export, delete | `adapters::ffmpeg` | `MemFiles` |
| `LiveStreams` | start, update and stop a playlist's stream; now, next and listeners (arc42 8.4) | `adapters::live` | `FakeLive` |
| `LiveListening` | a listener's bytes of a running stream, the title on air, the HLS folder | `adapters::live` | `FakeListening` |
| `Relay` | MediaMTX: who is asking its auth hook, its state, the listener addresses (arc42 8.4) | `adapters::live` | `FakeRelay` |
| `Notifier` | tell the browser what changed (server-sent events) | `web::events` | `MemNotifier` |
| `Clock`, `Ids` | time and ids, so use cases are testable | `adapters::system` | `FixedClock`, `SeqIds` |

## 5.3 Level 3: inside the bigger adapters

```c4
live
```

**`adapters::live`** (the live playout, arc42 8.4):

| Part | File | Does |
|------|------|------|
| Stations | `live/mod.rs` | One station per live playlist, behind `LiveStreams`, `LiveListening` and `Relay`: starts its playout and encoders, answers now, next and listeners, hands out listeners' streams, gives MediaMTX's status. |
| Playout | `live/station.rs` | Picks songs in the live order (`domain::live::PlayOrder`), decodes each while it plays, paces 20 ms PCM chunks onto a bus; remembers the song on air for restarts. |
| Mixer | `live/mixer.rs` | Two decks with an equal-power crossfade. |
| Encoders | `live/station.rs` | One ffmpeg each for MP3, AAC, HLS and the relay, fed from the PCM bus. |
| Relay | `live/relay.rs` | Publishes AAC to MediaMTX per live path, tells Maio's secret from anyone else's for the auth hook (what each may do is `domain::relay::allowed`), checks its control API every 5 s (version, paths, readers), kicks readers after a move. |

```c4
webapp
```

**`ui`** (the web app, arc42 8.5): `frame` (header, engine and MediaMTX panels), `songs` (Songs screen, Playlists screen, shelf), `playlist` (a playlist's page and its dialogs), `live` (the live strip), `player` (one `<audio>` for the whole app), `details` (a song's details), `modal` (the native dialog), `api` (the JSON client), `events` (the store: the latest state each server-sent event told, by id), `format`, `queue`, `shelf`, `make` (pure helpers, unit-tested).
