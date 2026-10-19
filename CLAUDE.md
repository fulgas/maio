# Maio

`SPEC.md` is the authority. Read the section you are touching before changing code.

## Principles (SPEC 3), applied to every change

- **SOLID**: one reason to change per module; ports are small, role-specific traits; use cases depend on traits, never on adapters; adapters are swapped in tests with in-memory fakes.
- **KISS**: the simplest thing that meets the spec. One binary, one SQLite file, no brokers, no plugin systems.
- **YAGNI**: nothing for a second user, a second engine kind or a future feature until the spec asks for it. No abstraction with one implementation, no config for a value that never changes.

## Layout

One crate, `maio`, one module per layer: `domain` ← `usecases` ← `adapters` ← `web`, plus `ui` (the Leptos web app, which uses only `domain` and talks to the server through `/api/v1`). `tests/layers.rs` enforces the direction.

## Working here

- Test first: `just test` (fmt check, every test, the wasm build) and `just e2e` (a real browser against the fake engine) stay green.
- Never print `.env` values. Commit only on a branch; merging is the user's call.
