# Maio

Makes songs with music generation models, keeps them in your library, builds playlists from them and plays them.
Spec: `SPEC.md`. The ACE-Step engine server is the `maio-engine` repo, next to this one.

## Run

1. `cp maio.example.toml maio.toml && cp .env.example .env`, then set `MAIO_ADMIN_USER` and
   `MAIO_ADMIN_PASSWORD` (10 characters or more). They make the one account on first start.
2. `just` starts Maio and MediaMTX in Docker and runs ACE-Step in this terminal; Ctrl-C stops both.
   `just fake` runs a fake engine (sine tones, no GPU) instead of ACE-Step.
3. The API is at http://localhost:8080/api/v1 (SPEC 10). The web app arrives in a later version.

## Scripts

Put `MAIO_API_TOKEN` (32 characters or more) in `.env` and send it as a bearer token:

```bash
curl -H "Authorization: Bearer $MAIO_API_TOKEN" -H 'content-type: application/json' \
  -d '{"prompt": "warm pads, slow", "mood": "calm"}' http://localhost:8080/api/v1/songs
```

## Develop

`cargo test` (needs `ffmpeg` and `ffprobe` on PATH).
