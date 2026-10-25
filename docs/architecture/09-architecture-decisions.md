# 9. Architecture decisions

Decisions that still shape Maio are recorded as architecture decision records (ADRs), one file each in `docs/adr/`. This section is their index.

- Each record follows [MADR 4.0.0](https://adr.github.io/madr/): front matter (`status`, `date`, `decision-makers`), then Context and Problem Statement, Considered Options, Decision Outcome with Consequences and Confirmation ([ADR 0013](../adr/0013-madr-decision-records.md)). Start a new one from the [template](../adr/template.md).
- A decision that is replaced keeps its file, with `status: superseded by ADR NNNN`, and the new record says which one it replaces.
- A decision that would surprise the next reader gets an ADR ([project rules](../project-rules.md)).

| ADR | Decision | Status |
| --- | --- | --- |
| [0001](../adr/0001-songs-and-playlists-replace-the-radio.md) | Songs and playlists replace the radio | Accepted |
| [0002](../adr/0002-a-hexagon-in-one-crate.md) | A hexagon in one crate, its direction tested | Accepted |
| [0003](../adr/0003-one-binary-one-sqlite-file.md) | One binary, one SQLite file, no broker | Accepted |
| [0004](../adr/0004-the-web-app-uses-only-the-public-api.md) | Leptos SSR; the web app uses only the public API | Accepted |
| [0005](../adr/0005-one-account-sessions-and-an-api-token.md) | One account, server-side sessions and an API token | Accepted |
| [0006](../adr/0006-ace-steps-own-api.md) | ACE-Step's own API, no wrapper | Accepted |
| [0007](../adr/0007-live-playout-on-the-server-mediamtx-as-relay.md) | Live playout on the server; MediaMTX as the relay | Accepted |
| [0008](../adr/0008-songs-are-kept-opus-by-default.md) | Songs are kept; Opus by default, loudness-normalised | Accepted |
| [0009](../adr/0009-opentelemetry-across-maio-and-ace-step.md) | OpenTelemetry across Maio and ACE-Step | Accepted |
| [0010](../adr/0010-native-browser-features-first.md) | Native browser features first | Accepted |
| [0011](../adr/0011-a-live-playlist-plays-its-stream-in-the-player.md) | A live playlist plays its stream in the player | Accepted |
| [0012](../adr/0012-arc42-is-the-authority.md) | arc42 is the authority | Accepted |
| [0013](../adr/0013-madr-decision-records.md) | Decision records follow MADR 4.0.0 | Accepted |
| [0014](../adr/0014-product-design-and-tech-pages.md) | Product, Design and Tech pages lead into arc42 | Accepted |
| [0015](../adr/0015-conventional-commits-decide-semantic-versions.md) | Conventional Commits decide semantic versions | Accepted |
| [0016](../adr/0016-zola-builds-the-documentation-site.md) | Zola builds the documentation site | Accepted |
| [0017](../adr/0017-the-engine-health-is-the-circuit-breaker.md) | The engine's health is the circuit breaker; trouble is retried with exponential backoff | Accepted |
| [0018](../adr/0018-the-web-app-learns-of-changes-from-events.md) | The web app learns of changes from server-sent events, never by polling | Accepted |
