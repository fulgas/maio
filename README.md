# Maio

Makes songs with music generation models, keeps them in your library, builds playlists from them and plays them.
- [Product](docs/product.md), [Design](DESIGN.md) and [Tech](docs/tech.md): what Maio is, how it looks, how it is built, each on one page.
- [Architecture](docs/architecture/01-introduction-and-goals.md): twelve arc42 sections, the authority for what Maio does and how it is built.
- [Decisions](docs/architecture/09-architecture-decisions.md): arc42 section 9, the index of the ADRs and their reasons.
- [Project rules](docs/project-rules.md): how changes are made.

`just docs-serve` shows all of it as a website with an interactive C4 explorer at http://localhost:8000;
`docs-site/dist/architecture.html` is the same site in one file.

## Run

1. `cp maio.example.toml maio.toml && cp .env.example .env`, then set `MAIO_ADMIN_USER` and
   `MAIO_ADMIN_PASSWORD` (10 characters or more). They make the one account on first start.
2. `just` starts Maio and MediaMTX in Docker and runs ACE-Step in this terminal; Ctrl-C stops both.
   `just fake` runs a fake engine (sine tones, no GPU) instead of ACE-Step.
3. Open http://localhost:8080 and sign in. Everything the web app does, the API at http://localhost:8080/api/v1
   does too (arc42 3.5).
4. Press **Live** on a playlist to stream it: the strip shows MP3, AAC and HLS links (and RTSP, SRT, RTMP through
   MediaMTX), whether it is public or private, and who is listening.

## Scripts

Put `MAIO_API_TOKEN` (32 characters or more) in `.env` and send it as a bearer token:

```bash
curl -H "Authorization: Bearer $MAIO_API_TOKEN" -H 'content-type: application/json' \
  -d '{"prompt": "warm pads, slow", "mood": "calm"}' http://localhost:8080/api/v1/songs
```

## Develop

`cargo test` (needs `ffmpeg` and `ffprobe` on PATH).

`just dev` runs the web app with live reload (`cargo leptos watch`; needs Rust 1.88 or later,
`rustup target add wasm32-unknown-unknown`, `cargo install cargo-leptos@0.3.5` and a local `maio.toml`
with `listen = "127.0.0.1:8080"`). `just test` runs every test and the browser build; `just e2e` drives a real
browser through every screen against the fake engine (needs Node).

## Traces

Set `OTEL_EXPORTER_OTLP_ENDPOINT` (for example `http://localhost:4318`) for Maio and for `just engine`:
Maio sends traces, metrics and logs over OTLP, and ACE-Step runs under OpenTelemetry's zero-code
instrumentation, so one trace holds your request, the job and ACE-Step's own spans (arc42 8.6).
