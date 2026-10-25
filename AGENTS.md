# AGENTS.md

Instructions for coding agents working on Maio. People start with [README.md](README.md) and [CONTRIBUTING.md](CONTRIBUTING.md); everything here is also true for them.

## What this is

Maio makes songs with ACE-Step 1.5, keeps them in a library, arranges them into playlists and plays them, in the browser or as live streams. One Rust crate, one binary, one SQLite file; MediaMTX beside it in Docker; ACE-Step on a machine with a GPU.

## Where the truth is

- **`docs/architecture/`** (twelve arc42 sections) is the authority on what Maio does and how it is built. Read the section you are touching before changing code; code points to it as `arc42 x.y`.
- **`docs/project-rules.md`** says how changes are made. Where it and this file differ, it wins.
- **`docs/product.md`**, **`DESIGN.md`** and **`docs/tech.md`** are the one-page versions. `DESIGN.md` owns the web app's look, in the [DESIGN.md format](https://github.com/google-labs-code/design.md/blob/main/docs/spec.md); `style/main.css` follows its tokens.
- **`docs/adr/`** holds the decision records, in MADR 4.0.0, indexed by arc42 section 9.

## Commands

| Command | Does |
|---------|------|
| `just test` | Format check, every Rust test, the WebAssembly build, the documentation checks. Run before every commit. |
| `just e2e` | A real browser through every screen against a fake ACE-Step. Run for any change to the web app, the API or the streams. |
| `just dev` | The web app with live reload. |
| `just stack`, `just fake` | Maio and MediaMTX in Docker, and a fake engine instead of ACE-Step. |
| `just docs`, `just docs-serve` | Build the documentation website; serve it at http://localhost:8000. |

`cargo test` needs `ffmpeg` and `ffprobe` on PATH. `just e2e` takes minutes: run it in the background.

## Code

- **Layers**: one crate, one module per layer: `domain` ← `usecases` ← `adapters` ← `web`, plus `ui`, the Leptos web app, which uses only `domain` and reaches the server through `/api/v1`. Dependencies point inward. `tests/layers.rs` fails the build when a layer uses one it must not, or when a `web` file other than `start.rs` and `config.rs` names an adapter.
- **Hexagon**: rules live in `domain`; one use case per thing a person does, in `usecases`; ports are the use cases' own small traits, in `src/usecases/ports.rs`, each with an in-memory fake in `src/usecases/testing.rs`; adapters implement ports. A request handler only translates HTTP to a use case call; it holds no rule and no adapter.
- **SOLID, KISS, YAGNI** (arc42 2.3): one reason to change per module; the simplest thing that meets the documentation; nothing for a second user, a second engine kind or a future feature until the documentation asks for it. No abstraction with one implementation, no config for a value that never changes.
- **Proven crates first**: a solved problem (retries and backoff, parsing, dates, encoding) uses a well-used crate that does that job, not code of our own.
- **Web app**: no polling and no fetching to catch up: every change the server sees is told at its source as an event carrying the new state (`web/push.rs`), and the web app updates only what shows it, from its store (`ui/events.rs`). Native browser features first: `<dialog>` through the `Modal` component, the popover API for panels. Actions are icon buttons with a caption (`data-tip`) and an accessible name. WCAG 2.1 AA: keyboard throughout, visible focus, text contrast of 4.5:1 or more, targets of 24 px or more (44 px for icon buttons), from 360 px wide.

## Making a change

1. Read the arc42 section first. A change to what Maio does changes that section in the same commit, or before.
2. Write the failing test first and watch it fail for the right reason, then make it pass. A fix to the web app gets a check in `e2e/smoke.mjs`.
3. A decision that would surprise the next reader gets a record started from `docs/adr/template.md`, and a row in section 9 with its status.
4. Keep `just test` and `just e2e` green.

## Documentation rules

- The documentation describes Maio only: no references to the tools or assistants used to write it.
- A decision record is words only: what was decided and why, with no code, file paths, commands, config keys or HTML. The details go in the arc42 section it points to; a test checks.
- Structure (context, containers, components, deployment) is drawn by the C4 model in `docs-site/static/model.mjs` and embedded with a ` ```c4 <view> ` block; Mermaid draws sequence and state diagrams only, and never contains a semicolon (Mermaid reads it as the end of a statement). A folder tree is a ` ```tree ` block.
- Every colour in `style/main.css` has a token in `DESIGN.md`, and every `just` recipe is in `docs/tech.md`; tests check both.
- State only what the code or the configuration confirms. Check a value in the source before writing it down.

## Git

- Commit only on a branch, never on `main`. Merging and every push are the user's call.
- Commits are made as `fulgas <2473927+fulgas@users.noreply.github.com>` and signed with the fulgas key (`33769175F7EF2CCD`); the repository's local git config sets both.
- Commit messages follow [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/): `type(scope)!: what changed, for the person using Maio`, with the types and scopes in `docs/project-rules.md`, one logical change per commit, and a `BREAKING CHANGE:` footer for anything that breaks the API, the config or stored data.
- Every commit is a Conventional Commit; `.githooks/commit-msg` refuses anything else (`git config core.hooksPath .githooks`).
- Releases follow Semantic Versioning, the number worked out from the commits since the last release, as `docs/project-rules.md` sets out; a release is a signed `vX.Y.Z` tag on `main` matching `Cargo.toml`.
- An assistant is credited the way the Linux kernel does it ([AI Coding Assistants](https://docs.kernel.org/process/coding-assistants.html)): an `Assisted-by: AGENT_NAME:MODEL_VERSION [TOOL1] [TOOL2]` trailer, such as `Assisted-by: Claude:claude-opus-5-5`, listing only specialised analysis tools, never git, compilers or editors. An assistant is never a co-author and never signs; the person who commits reviews the change and answers for it. No session or conversation links.

## Secrets

Secrets live in `.env` and are never printed, logged or committed: the account password, the API token, engine keys. Read them into variables when a script needs them; never echo them. Logs name songs, playlists, jobs and engines by id.
