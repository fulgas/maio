# 2. Architecture constraints

## 2.1 Technical

| Constraint | Why |
|------------|-----|
| Rust 2024, one crate `maio` | One build, one binary; layers are modules, not crates ([ADR 0002](../adr/0002-a-hexagon-in-one-crate.md)). |
| Leptos 0.8 with SSR and hydration, thaw for dialogs' looks and toasts, axum underneath | The web app is Rust too and renders on the server ([ADR 0004](../adr/0004-the-web-app-uses-only-the-public-api.md)). |
| SQLite (rusqlite), one file in the data volume | Nothing to run beside it ([ADR 0003](../adr/0003-one-binary-one-sqlite-file.md)). |
| ffmpeg in the container | Import, loudness normalisation, tags, export, decoding for live, the live encoders. |
| ACE-Step 1.5 on the Mac, through its own `acestep-api` | Music generation needs Metal; the container cannot have it ([ADR 0006](../adr/0006-ace-steps-own-api.md)). |
| MediaMTX beside Maio | RTSP, SRT and RTMP without writing them ([ADR 0007](../adr/0007-live-playout-on-the-server-mediamtx-as-relay.md)). |
| Docker Compose | Maio and MediaMTX in their own compose project and network. |
| Browsers | Current evergreen browsers; native `<dialog>` and popover are relied on ([ADR 0010](../adr/0010-native-browser-features-first.md)). |

## 2.2 Organisational

- One person owns and runs it: no second user, no roles, no sign-up ([ADR 0005](../adr/0005-one-account-sessions-and-an-api-token.md)).
- Test first: `just test` (format check, every test, the browser build) and `just e2e` (a real browser against a fake ACE-Step) stay green.
- Commits happen on a branch; merging is the owner's call. Secrets live in `.env` and are never printed.

## 2.3 Principles, enforced in review

- **SOLID**: one reason to change per module; ports are small, role-specific traits; use cases depend on traits, never on adapters; adapters are swapped in tests with in-memory fakes.
- **KISS**: the simplest thing that meets this documentation. One binary, one SQLite file, no message brokers, no plugin systems.
- **YAGNI**: nothing for a second user, a second engine kind or a future feature until this documentation asks for it. No abstraction with one implementation, no config for a value that never changes.
- **Clean code**: names from this documentation's vocabulary (song, playlist, live, prompt; [glossary](12-glossary.md)), small functions, no dead code, errors as typed domain errors mapped to HTTP status codes at the edge.
