# Tech

How Maio is built and worked on. The [architecture](architecture/01-introduction-and-goals.md) is the authority; this page points into it.

## Stack

| Part | What | Where it is described |
|------|------|-----------------------|
| Language | Rust 2024, one crate `maio` | [arc42 2.1](architecture/02-architecture-constraints.md#21-technical) |
| Web | axum; Leptos 0.8 with SSR and hydration; thaw for looks and toasts | [ADR 0004](adr/0004-the-web-app-uses-only-the-public-api.md) |
| Library | SQLite (rusqlite), one file in the data volume | [ADR 0003](adr/0003-one-binary-one-sqlite-file.md) |
| Audio | ffmpeg: normalisation, tags, export, the live encoders | [arc42 8.4](architecture/08-crosscutting-concepts.md) |
| Music engine | ACE-Step 1.5, its own server (`acestep-api`) on a machine with a GPU | [arc42 3.3](architecture/03-system-scope-and-context.md#33-the-music-engine-ace-step-15) |
| Relay | MediaMTX for RTSP, SRT and RTMP | [ADR 0007](adr/0007-live-playout-on-the-server-mediamtx-as-relay.md) |
| Run | Docker Compose: Maio and MediaMTX | [arc42 7](architecture/07-deployment-view.md) |
| Documentation site | Zola 0.23.6, fed by `docs-site/sync.mjs`; our C4 explorer | [ADR 0016](adr/0016-zola-builds-the-documentation-site.md) |
| Checks on GitHub | GitHub Actions: `just test`, `just e2e`, clippy for the server and the web app, `cargo deny` (licences, advisories, sources) and a weekly `cargo audit`; each push to `main` publishes the documentation site to https://fulgas.github.io/maio/ | `.github/workflows` |
| Engine calls | reqwest; backon for exponential backoff | [arc42 8.8](architecture/08-crosscutting-concepts.md#88-when-the-engine-is-busy-or-down) |
| Shared state | tokio `watch` channels for the latest value an adapter knows (an engine's health, the song on air, MediaMTX's state); parking_lot for the few locks left (the SQLite connection, and the queues and counters many requests change at once), since its locks are never poisoned | [arc42 8.5](architecture/08-crosscutting-concepts.md#85-the-web-app) |
| Telemetry | OpenTelemetry over OTLP | [ADR 0009](adr/0009-opentelemetry-across-maio-and-ace-step.md) |

## Code

```tree
src/
  domain/      types and rules
  usecases/    use cases and their port traits
  adapters/    SQLite, engines, ffmpeg, live, relay
  web/         the axum server; wires everything
  ui/          the Leptos web app
tests/         Rust tests; layers.rs keeps the direction
e2e/           a real browser against a fake acestep-api
docs/          this documentation
docs-site/     the documentation website and the C4 model
```

The layers and their ports: [arc42 5](architecture/05-building-block-view.md).

## Commands

| Command | Does |
|---------|------|
| `just run` | The default (`just`): starts the stack, then ACE-Step in this terminal; Ctrl-C stops both. |
| `just stack` | Starts Maio and MediaMTX in Docker, rebuilding when the code changed. |
| `just engine` | On a Mac with Apple silicon, runs ACE-Step's own API (MPS), with OpenTelemetry when configured. |
| `just fake` | Runs a fake ACE-Step (sine tones, no GPU). |
| `just stop` | Stops the stack. |
| `just logs` | Follows Maio's log. |
| `just dev` | The web app with live reload (`cargo leptos watch`). |
| `just test` | Format check, every Rust test, the WebAssembly build, the documentation checks. |
| `just e2e` | A real browser through every screen against the fake engine. |
| `just zola` | Downloads the pinned Zola into `docs-site/.zola`, once. |
| `just docs` | Builds the documentation website into `docs-site/dist` with Zola. |
| `just docs-serve` | Serves it at http://localhost:8000. |

How changes are made (test first, Conventional Commits, secrets): [project rules](project-rules.md).
