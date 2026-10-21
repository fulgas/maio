# Maio

The arc42 sections in `docs/architecture/` are the authority; `docs/project-rules.md` says how changes are made. `docs/product.md`, `DESIGN.md` and `docs/tech.md` are the one-page versions; `DESIGN.md` owns the web app's look, in the DESIGN.md format, and `style/main.css` follows its tokens. Read the section you are touching before changing code; code points to it as `arc42 x.y`. A change to what Maio does changes that section first; a decision that would surprise the next reader gets an ADR in `docs/adr`.

## Principles (arc42 2.3), applied to every change

- **SOLID**: one reason to change per module; ports are small, role-specific traits; use cases depend on traits, never on adapters; adapters are swapped in tests with in-memory fakes.
- **KISS**: the simplest thing that meets the documentation. One binary, one SQLite file, no brokers, no plugin systems.
- **YAGNI**: nothing for a second user, a second engine kind or a future feature until the documentation asks for it. No abstraction with one implementation, no config for a value that never changes.

## Layout

One crate, `maio`, one module per layer: `domain` ← `usecases` ← `adapters` ← `web`, plus `ui` (the Leptos web app, which uses only `domain` and talks to the server through `/api/v1`). `tests/layers.rs` enforces the direction.

## Working here

- Test first: `just test` (fmt check, every test, the wasm build) and `just e2e` (a real browser against the fake engine) stay green.
- Never print `.env` values.
- Commit only on a branch; merging and every push are the user's call.
- Commits are made as `fulgas <2473927+fulgas@users.noreply.github.com>` and signed with the fulgas key (`33769175F7EF2CCD`); the repo's local git config sets both.
- Commit messages follow Conventional Commits 1.0.0 as `docs/project-rules.md` sets out: `type(scope)!: what changed, for the person using Maio`, a `BREAKING CHANGE:` footer for anything that breaks the API, config or stored data.
