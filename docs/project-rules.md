# Project rules

How changes to Maio are made. What Maio does and how it is built is in the [architecture](architecture/01-introduction-and-goals.md); the [decisions](architecture/09-architecture-decisions.md) say why.

## The architecture comes first

- The arc42 sections in `docs/architecture/` are the authority. Read the section you are touching before changing code; code points to it as `arc42 x.y`.
- A change to what Maio does changes its section first, then the code.
- A decision that would surprise the next reader gets a record in `docs/adr/`, in [MADR 4.0.0](https://adr.github.io/madr/), started from the [template](adr/template.md), and a row in [section 9](architecture/09-architecture-decisions.md). A replaced decision keeps its file, with `status: superseded by ADR NNNN`. A record says what was decided and why, in words: no code, file paths, commands, config keys or HTML; those belong in the arc42 section it points to.
- The C4 model (`docs-site/static/model.mjs`) changes with the building blocks it draws.

## Principles

SOLID, KISS and YAGNI, as [arc42 2.3](architecture/02-architecture-constraints.md#23-principles-enforced-in-review) spells them out: small ports and use cases that depend on them, the simplest thing that meets the documentation, and nothing for a future that has not been asked for. The layers keep their direction (`domain` ← `usecases` ← `adapters` ← `web`, plus `ui`); `tests/layers.rs` fails the build otherwise.

## Test first

- A change starts with a test that fails for the right reason, then the code that makes it pass.
- `just test` (format check, every Rust test, the WebAssembly build, the documentation site's checks) and `just e2e` (a real browser through every screen against a fake ACE-Step) stay green.
- A fix to the web app gets an e2e check that fails without it.

## Conventional Commits

Commit messages follow [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/):

```text
type(optional-scope)!: what changed, for the person using Maio

Optional body: why, and what it affects.

BREAKING CHANGE: what no longer works, and what to do instead
```

The subject says what changed in plain words (`fix(player): Pause while listening live turns to Play`), not which file was edited. The `!` and the `BREAKING CHANGE:` footer mark a change that breaks the API, the config or stored data; a breaking change always has the footer.

| Type | Use |
| --- | --- |
| `feat` | Something new people can use. |
| `fix` | A defect corrected. |
| `docs` | Documentation only: the arc42 sections, ADRs, these rules, the C4 model, README. |
| `test` | Tests only, no change in behaviour. |
| `refactor` | Code reshaped with no change in behaviour. |
| `perf` | Faster or lighter, same behaviour. |
| `build`, `ci`, `chore`, `style`, `revert` | Build and dependencies, automation, upkeep, formatting, undoing earlier work. |

Scopes, when they help: `songs`, `playlists`, `player`, `live`, `relay`, `engine`, `text`, `account`, `api`, `ui`, `config`, `telemetry`, `docs-site`. One logical change per commit.

Every commit follows it. The `commit-msg` hook in `.githooks/` refuses any other message; turn it on once per clone with `git config core.hooksPath .githooks`. Every change reaches `main` through a pull request, where the checks run; `just test`, clippy and the DCO check must pass first. Every commit is also signed off (`git commit -s`): its `Signed-off-by:` line certifies the [Developer Certificate of Origin](https://developercertificate.org), and a DCO check on pull requests refuses commits without one.

## Releases

Releases follow [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html), and the commits decide the number ([ADR 0015](adr/0015-conventional-commits-decide-semantic-versions.md)). Since the last release:

| The commits include | The next version |
| --- | --- |
| a `BREAKING CHANGE:` footer or a `!` after the type | major (while the version is 0.x, minor) |
| a `feat` | minor |
| only `fix` or `perf` | patch |
| nothing else (`docs`, `test`, `refactor`, `build`, `ci`, `chore`, `style`) | no release |

A release comes through a pull request ([ADR 0019](adr/0019-releases-come-through-a-release-pull-request.md)). On every push to `main`, release-plz (configured in `release-plz.toml`) keeps one pull request open, titled `chore(release): vX.Y.Z`: it sets `version` in `Cargo.toml` and adds the release to `CHANGELOG.md`, its notes made from the commit subjects grouped as Features, Fixes, Performance and Reverts. Only `feat`, `fix`, `perf`, `revert` and breaking commits open one; a commit scoped `docs-site` never makes a release or a line in the changelog, since the documentation site is not part of Maio. Merging it is the release: the release app tags `vX.Y.Z` on `main` and makes the GitHub Release as a draft (`release.yml`). The tag starts two workflows, each of which can also be run by hand for a tag. `binaries.yml` attaches `maio` with its web app for `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` and `aarch64-apple-darwin`, each with a SHA-256 checksum, a CycloneDX SBOM and a Sigstore signature bundle, and only then publishes the release, since releases are immutable once published. `image.yml` publishes the container image `ghcr.io/fulgas/maio` for `linux/amd64` and `linux/arm64`, tagged `X.Y.Z`, `X.Y` and `latest`, with an SPDX SBOM and build provenance, and signs it by its digest with cosign; the image is not part of the release, so the release never waits for it. Both signatures are keyless: Sigstore certifies the identity of the workflow that built each one, so there is no signing key to keep (the README says how to verify them). v0.1.0 was published without its files and withdrawn; GitHub never lets an immutable release's tag name be used again, so the first release is v0.1.1.

## Secrets

- Secrets live in `.env` and are never printed, logged or committed: the account password, the API token, engine keys.
- Logs name jobs, songs, playlists and engines by id, never a secret ([arc42 8.6](architecture/08-crosscutting-concepts.md#86-logs-traces-and-metrics)).

## Accessibility

Every screen meets WCAG 2.1 AA ([arc42 10](architecture/10-quality-requirements.md)): keyboard throughout, visible focus, text contrast of at least 4.5:1, targets of at least 24 px, from 360 px wide. A review with axe-core and a keyboard pass comes before a release.
