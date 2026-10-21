# 4. Solution strategy

| Goal (1.3) | Approach |
|------------|----------|
| Simple to run | One Rust binary serves the web app, the API and the streams; SQLite for the library; ffmpeg for audio; MediaMTX only for the protocols Maio does not speak ([ADR 0003](../adr/0003-one-binary-one-sqlite-file.md), [ADR 0007](../adr/0007-live-playout-on-the-server-mediamtx-as-relay.md)). |
| Changeable | A hexagon: domain rules, use cases behind small port traits, adapters at the edge, all in one crate with the direction tested ([ADR 0002](../adr/0002-a-hexagon-in-one-crate.md)). |
| Never open | An account from the environment or no start; server-side sessions or one API token; `Origin` checks; secrets kept out of logs ([ADR 0005](../adr/0005-one-account-sessions-and-an-api-token.md)). |
| Usable by everyone | The browser's own `<dialog>` and popover, real buttons and labels, a live region for toasts, checked by axe and a keyboard pass ([ADR 0010](../adr/0010-native-browser-features-first.md)). |
| A stream that keeps going | Playout on the server with its own mixer and encoders; the song on air remembered across restarts; MediaMTX watched every 5 s and shown in the header. |

Key choices:

- **The web app is a client of the public API.** Leptos renders on the server and hydrates; in the browser it calls `/api/v1` like any script ([ADR 0004](../adr/0004-the-web-app-uses-only-the-public-api.md)).
- **ACE-Step is used as it is.** Maio speaks `acestep-api` directly instead of wrapping it ([ADR 0006](../adr/0006-ace-steps-own-api.md)).
- **Songs are kept, never swept.** No storage budget; Opus by default, loudness-normalised ([ADR 0008](../adr/0008-songs-are-kept-opus-by-default.md)).
- **One trace across Maio and ACE-Step.** OpenTelemetry over OTLP, `traceparent` on engine calls ([ADR 0009](../adr/0009-opentelemetry-across-maio-and-ace-step.md)).
- **A live playlist plays its stream in the player.** You hear what listeners hear ([ADR 0011](../adr/0011-a-live-playlist-plays-its-stream-in-the-player.md)).
