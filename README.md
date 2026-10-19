# Maio

Makes songs with music generation models, keeps them in your library, builds playlists from them and plays them.
Spec: `SPEC.md`. The ACE-Step engine server is the `maio-engine` repo, next to this one.

## Run

1. `cp maio.example.toml maio.toml && cp .env.example .env`, then set `MAIO_ADMIN_USER` and
   `MAIO_ADMIN_PASSWORD` (10 characters or more). They make the one account on first start.
2. `just` starts Maio and MediaMTX in Docker and runs ACE-Step in this terminal; Ctrl-C stops both.
   `just fake` runs a fake engine (sine tones, no GPU) instead of ACE-Step.
3. Open http://localhost:8080 and sign in. Everything the web app does, the API at http://localhost:8080/api/v1
   does too (SPEC 10).
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
