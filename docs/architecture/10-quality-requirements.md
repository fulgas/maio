# 10. Quality requirements

## 10.1 Quality scenarios

| Goal | Scenario | Expected |
|------|----------|----------|
| Simple to run | A fresh Mac with Docker and the ACE-Step checkout | `cp` the example config and `.env`, set the account, `just`: the web app on 8080 and ACE-Step loading. |
| Changeable | A use case imports an adapter | `tests/layers.rs` fails the build. |
| Changeable | A port gets a second adapter or a fake | No use case changes. |
| Never open | Start without `MAIO_ADMIN_USER` or `MAIO_ADMIN_PASSWORD` | Maio refuses to start. |
| Never open | A cookie request that changes something without a matching `Origin` | `403 bad_origin`. |
| Never open | Any log line at any level | No password, token, session id, API key or request header (`tests/logs.rs`). |
| Usable by everyone | Any screen, any theme, axe-core with the WCAG 2.1 AA tags | No violations. |
| Usable by everyone | Only the keyboard | Every control reachable with a visible focus ring; dialogs take focus, keep the page behind out of reach, and give focus back. |
| Usable by everyone | 360 px wide, or 200 % zoom | No sideways scrolling. |
| Usable by everyone | Any control | A target of at least 24 × 24 px and an accessible name; text contrast at least 4.5:1, non-text 3:1. |
| Stream keeps going | Maio restarts while a playlist is live | It comes back on air with the song that was on air. |
| Stream keeps going | MediaMTX stops | Within 5 s of the next look the header says "MediaMTX not answering", with a toast; when it is back Maio republishes and says so. |
| Stream keeps going | ACE-Step loses a task in a restart | The job fails as lost and is tried once more on another engine when there is one. |

## 10.2 How it is checked

- `just test`: format check, every unit and integration test (domain rules, use cases with fakes, adapters against SQLite, ffmpeg and fake engines, the HTTP API, pages, logs, traces), and the WebAssembly build.
- `just e2e`: a real browser through every screen against a fake `acestep-api` (`e2e/fake-ace.mjs`): making songs, playlists, live with private links, listening live, the accessibility checks (focus, targets, contrast, dialogs, captions inside the window, announcements), no console errors.
- `just test` also builds the architecture website and checks it: every link, every model reference into the documentation, every C4 view, and the model's geometry (`docs-site/tests`).
- An accessibility review before a release: axe-core on every screen and dialog in both themes, a keyboard pass, 200 % zoom.
