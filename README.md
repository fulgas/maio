# Maio

Makes music with generation models and streams it. Spec: `SPEC.md`. Engine server: `engine/`.

## Run

1. Start the ACE-Step engine server on the Mac (`engine/README.md`).
2. `cp maio.example.toml maio.toml && cp .env.example .env`, fill in `.env`. `MAIO_ADMIN_USER` and `MAIO_ADMIN_PASSWORD` (10 characters or more) create the first admin on first start; Maio refuses to start without them while it has no users.
3. `mkdir -p data && docker compose up -d --build` (Colima cannot create the bind-mount folder itself)
4. Open http://localhost:8080 and sign in as that admin. Add people under Users.

Signing in through the API needs an `Origin` header that matches the host (cross-site protection). There is no machine access: the API is for people who sign in, and anyone can listen through a radio's listen address.

Listen: http://localhost:8080/stream.mp3, /stream.aac or /hls/index.m3u8. The web player's Format menu picks one.

Storage: tracks are kept as Opus by default within a 20 GB budget (`[storage]` in `maio.toml`). When the budget is reached, Maio deletes the least recently played tracks you have not kept; if only kept tracks are left, new requests are refused until you empty the crate, and generative playlists replay their own tracks. Tracks in a fixed playlist are never deleted to make room.

## Playlists

Everything a radio plays is a playlist. The built-ins are Crate (every track), Kept (tracks you keep) and Requests
(tracks someone asked for once). People with the operator role make their own:

- fixed: tracks added by hand, played in their order or shuffled;
- generative: a recipe (`mood`, `prompt`, `seconds`, ...) that keeps making new tracks while it is on air;
- smart: a rule over the crate (`mood`, `genre`, `kept`, `side`, `from_playlist`, `max_seconds`, `made_within_days`).

Each playlist holds at most `max_tracks` (default 50, `[playlists]` in `maio.toml`). A generative playlist that reaches
its size loops the tracks it has. Put one on air with `PUT /api/v1/radios/<id>/on-air` and `{"playlist_id": "..."}`
(`null` takes it off air); it loops until you change it. Stations from earlier versions became generative playlists
with the same ids. The `station` rate limit in `[limits]` now limits how often someone switches the playlist on air.

## Song metadata

Every track has a title, an artist, an album and, for vocal songs, lyrics. Titles come in layers:

1. a person's rename: `PATCH /api/v1/library/<id>` with `{"title": "..."}`, never overwritten;
2. a text engine, when `[naming] engine` names one;
3. the first line of the lyrics' first `[chorus]`;
4. words picked by mood, genre and seed ("Low Tide Static").

The artist and album are the playlist that made the track, else Maio and Requests. Files carry these as tags, and
`/stream.mp3` and `/stream.aac` send the title to players that ask for it (VLC does). Vocal requests and recipes take
`vocals`, `vocal_language` and `lyrics_by` (`"ace-step"` or a text engine). Text engines are set up with `[[text_engine]]`
(Anthropic needs `ANTHROPIC_API_KEY` in `.env`; `openai` covers llama.cpp, Ollama, LM Studio and vLLM).

## Web UI

Open http://localhost:8080 and sign in. The Now screen has a radio dial: turn the knob, use its arrows, the mouse wheel
or the arrow keys to tune a radio; the player, queue and controls follow the radio you tuned, and your device remembers
it. The sun and moon button switches between light and dark. Playlists, Crate, Radios and (for admins) Users are the
other screens. `scripts/ui-preview.sh` runs a local preview with a fake engine and a few songs (`EMPTY=1` for a fresh
server), `scripts/screenshot.mjs` takes headless Chrome screenshots of it, and `node --test web/tests/*.test.mjs` tests
the page's helpers.

## Run with just

`just` starts the stack (Maio and MediaMTX) and runs ACE-Step in the same terminal; Ctrl-C stops both.
`just stack`, `just engine`, `just fake` (sine tones, no GPU), `just stop`, `just logs` and `just test` do one part each.

## Develop

```bash
cargo test
```

Tests need `ffmpeg` and `ffprobe` on PATH.

## Radios

Maio runs up to `max_radios` radios at once (default 4). `main` always exists. Admins add radios with
`POST /api/v1/radios` (`{"id": "lobby", "name": "Lobby"}`) until the web UI has a radio screen.

Each radio has a listen address that plays in any player (VLC, a browser, a phone radio app), no account needed:

- public: `http://localhost:8080/radios/<id>/stream.mp3` (also `.aac` and `hls/index.m3u8`)
- private: `http://localhost:8080/listen/<key>/stream.mp3`, where the key is secret; `POST /api/v1/radios/<id>/listen-key` makes a new one

`/stream.mp3`, `/stream.aac` and `/hls/index.m3u8` are the main radio's public address. Make a radio private with
`PATCH /api/v1/radios/<id>` and `{"visibility": "private"}`. Streams are plain HTTP.

The `mediamtx` service in `compose.yaml` also serves every radio over RTSP, SRT and RTMP, for VLC, OBS or broadcast tools:

- `rtsp://localhost:8555/radios/<id>`
- `srt://localhost:8891?streamid=read:radios/<id>`
- `rtmp://localhost:1936/radios/<id>`

Private radios use `listen/<key>` instead of `radios/<id>`. `GET /api/v1/radios` lists every address of every radio. The host ports
(8555, 8891, 1936) leave MediaMTX's usual ports free for another MediaMTX on the same machine; change them in `compose.yaml` and
`[relay]` together.
