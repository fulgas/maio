# Maio

**M**usic + **AI** + **O**utput.

Make songs with a music generation model, keep them in your own library, arrange them into playlists, and play them: in the browser, or as live streams anyone with the link can tune in to.

Maio is a personal, self-hosted tool. One person runs it and owns everything in it.

## Features

- **Songs from a prompt.** Describe a song, pick mood, genre, length and tempo, with or without vocals; [ACE-Step 1.5](https://github.com/ace-step/ACE-Step-1.5) makes it. Lyrics and titles can come from a text engine (Anthropic or any OpenAI-compatible API, such as Ollama or LM Studio).
- **A library you keep.** Songs stay until you delete them, loudness-normalised, stored as Opus by default; download any song as MP3, AAC, FLAC, WAV or Opus. Every song keeps its seed, so you can make another like it.
- **Playlists.** Ordered by drag and drop, with shuffle and repeat in the player.
- **Live streams.** Put a playlist on air: it plays on the server, with crossfades, as MP3, AAC and HLS, and through [MediaMTX](https://github.com/bluenviron/mediamtx) over RTSP, SRT and RTMP. Public or private links, listener counts, and it comes back on air after a restart.
- **An API for everything.** Whatever the web app does, a script can do through the JSON API with a token.
- **Accessible.** WCAG 2.1 AA: works with a keyboard and a screen reader, from 360 px wide, in a dark and a light theme.

## Requirements

- Docker with Compose, for Maio and MediaMTX.
- ACE-Step 1.5 on a machine with a GPU: CUDA is recommended; Apple silicon, AMD ROCm, Intel XPU and CPU work too. It can be this machine or another; set it up as [ACE-Step's own instructions](https://github.com/ace-step/ACE-Step-1.5#readme) say.
- [just](https://github.com/casey/just), to run the commands below.

## Quick start

1. Copy the config and the secrets file:

   ```bash
   cp maio.example.toml maio.toml
   cp .env.example .env
   ```

2. In `.env`, set `MAIO_ADMIN_USER` and `MAIO_ADMIN_PASSWORD` (10 to 128 characters). They create the one account on first start.
3. Run `just`. It starts Maio and MediaMTX in Docker, then ACE-Step in your terminal; Ctrl-C stops both. To try things without ACE-Step, run `just stack` and `just fake` (a fake engine that makes sine tones).
4. Open http://localhost:8080 and sign in.

## Releases

Each [release](https://github.com/fulgas/maio/releases) comes as the container image `ghcr.io/fulgas/maio` (for `linux/amd64` and `linux/arm64`) and as archives for Linux and for Macs with Apple silicon, each with its SBOM. The image and the archives are signed by the release workflow with [Sigstore](https://www.sigstore.dev), with no key to trust but the workflow's identity. To check them with [cosign](https://docs.sigstore.dev/cosign/system_config/installation/):

```bash
cosign verify ghcr.io/fulgas/maio:0.1.0 \
  --certificate-identity-regexp '^https://github.com/fulgas/maio/\.github/workflows/publish\.yml@refs/tags/v' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com

cosign verify-blob maio-v0.1.0-x86_64-unknown-linux-gnu.tar.gz \
  --bundle maio-v0.1.0-x86_64-unknown-linux-gnu.tar.gz.sigstore.json \
  --certificate-identity-regexp '^https://github.com/fulgas/maio/\.github/workflows/publish\.yml@refs/tags/v' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
```

## Engines and API keys

### ACE-Step, the music engine

ACE-Step 1.5 runs as its own server, `acestep-api`, outside Maio's Docker Compose project, on whichever machine has the GPU. Install and start it as [ACE-Step's own instructions](https://github.com/ace-step/ACE-Step-1.5#readme) say: they cover CUDA, Apple silicon, AMD ROCm, Intel XPU and CPU. The first start downloads the models, about 9.4 GB, and takes a minute or two. Then point the `[[engine]]` `url` in `maio.toml` at it:

- **The same machine as Maio**: `http://host.docker.internal:7801`, as in the example config.
- **Another machine**: that machine's address, with ACE-Step listening on it rather than only on `127.0.0.1` (ACE-Step's `ACESTEP_API_HOST`).

On a Mac with Apple silicon there is a shortcut. With ACE-Step cloned beside this repository (or `ACE_ROOT` set to its folder) and `uv sync` run in it, `just engine` starts `acestep-api` on 127.0.0.1:7801 with the models loaded, on MPS, and `just`, with no recipe, starts the stack first.

The header's engine icon turns green when ACE-Step is ready.

To protect ACE-Step with a key:

1. Start ACE-Step with a key (its `ACESTEP_API_KEY`) and put the same key in `.env` as `ACE_STEP_API_KEY=<a long random string>`. `just engine` passes it to ACE-Step for you.
2. In `maio.toml`, uncomment `api_key_env = "ACE_STEP_API_KEY"` under `[[engine]]`, so Maio sends it.


### Text engines, for lyrics and titles

Without one, ACE-Step writes the lyrics and titles come from word lists. To use a language model instead, uncomment a `[[text_engine]]` in `maio.toml`:

- **Anthropic**: the `kind = "anthropic"` engine, with `ANTHROPIC_API_KEY=<your key>` in `.env`.
- **A local model** (Ollama, LM Studio, llama.cpp, vLLM): the `kind = "openai"` engine, with `url` set to its OpenAI-compatible address as seen from Docker, such as `http://host.docker.internal:11434/v1` for Ollama, and `model` set to a model it serves.

Then choose it for lyrics with `[lyrics] default_engine` and for titles with `[naming] engine`. Each text engine's `api_key_env` names the `.env` variable that holds its key. The options are in [arc42 3.4](docs/architecture/03-system-scope-and-context.md#34-text-engines).

### After a change

Maio reads `.env` and `maio.toml` when it starts. After changing either, restart it:

```bash
docker compose up -d --force-recreate maio
```

and restart `just engine` after changing ACE-Step's key.

## Ports

| Port | What |
|------|------|
| 8080 | The web app, the JSON API at `/api/v1`, and the live streams (MP3, AAC, HLS) |
| 8555 | Live streams over RTSP (MediaMTX) |
| 8891/udp | Live streams over SRT (MediaMTX) |
| 1936 | Live streams over RTMP (MediaMTX) |

ACE-Step listens on port 7801 wherever it runs. MediaMTX's control API stays inside Docker.

## Configuration

- **`maio.toml`**: the moods and genres offered, the engines, how many playlists may be live at once, the crossfade, which stream formats are served, and the relay. Every setting is described in [arc42 7.3](docs/architecture/07-deployment-view.md#73-config).
- **`.env`**: secrets, never committed.

| Variable | What |
|----------|------|
| `MAIO_ADMIN_USER`, `MAIO_ADMIN_PASSWORD` | The account, created on first start. |
| `MAIO_API_TOKEN` | For scripts, at least 32 characters; leave empty for none. |
| `ANTHROPIC_API_KEY` | For an Anthropic text engine. |
| `ACE_STEP_API_KEY` | ACE-Step's API key, if you set one. |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | Send traces, metrics and logs to an OpenTelemetry collector. |

Served over HTTPS, set `secure_cookies = true`; behind a proxy that changes the host, set `public_origin`.

## Using the API

Send the token as a bearer token:

```bash
curl -H "Authorization: Bearer $MAIO_API_TOKEN" -H 'content-type: application/json' \
  -d '{"prompt": "warm pads, slow", "mood": "calm"}' http://localhost:8080/api/v1/songs
```

Every endpoint is in [arc42 3.5](docs/architecture/03-system-scope-and-context.md#35-the-http-api).

## Development

You need Rust (rustup installs the version in `rust-toolchain.toml`, with the `wasm32-unknown-unknown` target), `cargo-leptos` 0.3.5, `ffmpeg` and `ffprobe`, and Node 20 or later. `just` downloads the pinned [Zola](https://www.getzola.org) for the documentation site.

| Command | Does |
|---------|------|
| `just dev` | The web app with live reload. |
| `just test` | Format check, every Rust test, the WebAssembly build and the documentation checks. |
| `just e2e` | A real browser through every screen, against a fake engine. |
| `just docs-serve` | The documentation website at http://localhost:8000. |

The full list, the stack and the code layout are in [Tech](docs/tech.md).

## Documentation

- [Your first song, on air](docs/tutorials/01-your-first-song-on-air.md): a tutorial, start to finish.
- How-to guides: [run ACE-Step on another machine](docs/how-to/01-run-ace-step-on-another-machine.md), [add a text engine](docs/how-to/02-add-a-text-engine.md), [go live and share a stream](docs/how-to/03-go-live-and-share-a-stream.md), [serve Maio over HTTPS](docs/how-to/04-serve-maio-over-https.md), [back up the library](docs/how-to/05-back-up-the-library.md), [make a themed playlist with the API](docs/how-to/06-make-a-themed-playlist-with-the-api.md).
- [Reference](docs/reference.md): where to look up the API, the settings and the words used.
- [Product](docs/product.md), [Design](DESIGN.md) and [Tech](docs/tech.md): what Maio is, how it looks, how it is built, each on one page.
- [Architecture](docs/architecture/01-introduction-and-goals.md): twelve arc42 sections, the authority for what Maio does and how it is built.
- [Decisions](docs/architecture/09-architecture-decisions.md): arc42 section 9, the index of the decision records and their reasons.
- [Project rules](docs/project-rules.md): how changes are made.

`just docs-serve` shows all of it as a website with an interactive C4 explorer, built with Zola.

## Contributing

Read [Contributing](CONTRIBUTING.md) first. Report vulnerabilities privately, as [Security](SECURITY.md) explains. Everyone taking part follows the [Code of conduct](CODE_OF_CONDUCT.md).

## Credits

Maio stands on other people's work:

- [ACE-Step 1.5](https://github.com/ace-step/ACE-Step-1.5) makes the music; Maio talks to its own API server.
- [MediaMTX](https://github.com/bluenviron/mediamtx) relays the live streams over RTSP, SRT and RTMP.
- [FFmpeg](https://ffmpeg.org) decodes, normalises, tags and encodes every song and stream.
- [Leptos](https://leptos.dev) and [thaw](https://github.com/thaw-ui/thaw) build the web app; [axum](https://github.com/tokio-rs/axum) and [Tokio](https://tokio.rs) serve it; [SQLite](https://sqlite.org), through [rusqlite](https://github.com/rusqlite/rusqlite), keeps the library; [OpenTelemetry](https://opentelemetry.io) traces it.
- [Unbounded](https://fonts.google.com/specimen/Unbounded) and [Albert Sans](https://fonts.google.com/specimen/Albert+Sans) are the typefaces, under the SIL Open Font License (`public/fonts/*-OFL.txt`).
- [Zola](https://www.getzola.org) builds the documentation site, and [Mermaid](https://mermaid.js.org), MIT licensed and vendored in `docs-site/static/vendor`, draws its diagrams.
- The documentation follows [arc42](https://arc42.org), the [C4 model](https://c4model.com), [MADR](https://adr.github.io/madr/) and the [DESIGN.md format](https://github.com/google-labs-code/design.md); the code of conduct is the [Contributor Covenant](https://www.contributor-covenant.org).

## License

Either [Apache 2.0](LICENSE-APACHE.md) or [MIT](LICENSE-MIT.md), at your option. The bundled fonts keep their own licence, the SIL Open Font License 1.1.
