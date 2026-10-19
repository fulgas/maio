# Maio spec (v1)

Maio makes songs with music generation models, keeps them in your own library, lets you build playlists from them, and plays them, on demand in the browser or as live streams anyone with the link can tune in to.

It is a personal tool: one person runs it and owns everything in it. This version replaces v0 (radios, stations, queues and roles). The data from v0 is not carried over: v1 starts on a fresh database.

## 1. The model

| Thing | What it is |
|-------|------------|
| **Song** | Made from one prompt by a music engine, then kept in the library until you delete it. |
| **Playlist** | A named, ordered collection of songs you choose. A song can be in many playlists, or in none. Playlists have no prompts and make no songs themselves. |
| **Player** | Plays songs and playlists on demand in your browser, with shuffle, repeat one and repeat all. |
| **Live** | A switch on a playlist. While it is on, the playlist streams continuously, looping, to anyone with its link. |

Rules:

- One prompt makes one song. Making many songs from one prompt is a client convenience, not part of the server.
- A new song belongs to no playlist. You add it to playlists later.
- A song is kept until you delete it, and you can only delete a song that is in no playlist (`409 song_in_playlist`, naming the playlists). Nothing deletes songs automatically: there is no storage budget and no retention sweep.
- A playlist that is live cannot be deleted (`409 playlist_live`); take it off air first.
- At most `max_live` playlists are live at once (default 4): every live stream runs its own encoders. Going live beyond that is refused with `409 live_limit`, naming the live playlists.

## 2. Shape

```
 Docker (Maio's own compose and network)          Mac host / other machines
 ┌───────────────────────────────────────────┐
 │ browser ── HTTP ── maio (Rust)       │
 │                    ├ Leptos SSR web app   │
 │                    ├ JSON API             │──HTTP──> ACE-Step engine (Mac, Metal)
 │                    ├ library (SQLite)     │──HTTP──> other music engines
 │                    ├ live playout ─ ffmpeg│──HTTP──> text engines (Anthropic, OpenAI-compatible)
 │                    └ stream server        │
 │ mediamtx (RTSP, SRT, RTMP) <── publish ───┘
 └───────────────────────────────────────────┘
```

- Maio is one container: one binary plus ffmpeg, one config file (`maio.toml`), one data volume. MediaMTX runs beside it in the same compose file.
- Engine servers are separate programs Maio neither starts nor supervises. On the Mac, ACE-Step runs from a terminal (`just engine`).
- Port 8080 serves the web app, the JSON API and the live streams.

## 3. Architecture

Maio is written as a hexagon: a core that knows nothing about HTTP, SQLite, ffmpeg or Leptos, surrounded by adapters that plug into ports the core defines.

- **Domain** (`domain`): plain types and rules: `Song`, `Playlist`, `Live` state, titles in layers, the playlist rules above. No I/O, no async, no framework types. Every rule is tested here with plain unit tests.
- **Application** (`app`): one use case per user action (make a song, cancel a job, rename, delete, create a playlist, add, remove, reorder, go live, change visibility, new link). Each use case takes its ports as traits and returns domain results or domain errors.
- **Ports** (traits owned by the application):
  - `SongRepository`, `PlaylistRepository`: storage.
  - `MusicEngine`: submit, poll, download, cancel a generation (section 6).
  - `TextEngine`: write lyrics, name a song (section 7).
  - `AudioFiles`: import (transcode and normalise), tag, read duration, delete.
  - `LiveStreams`: start, stop and update the live stream of a playlist, report listeners.
  - `Clock` and `Ids`: time and id generation, so use cases are testable.
- **Adapters** (each behind one port): SQLite (rusqlite), the engine HTTP client (reqwest), the text engine clients, ffmpeg, the live playout and stream server, the MediaMTX relay, and the web layer.
- **Web layer**: Leptos with server-side rendering, served by axum. Pages render on the server and hydrate in the browser; Leptos server functions call the use cases. The JSON API (section 8) calls the same use cases for scripts. No business rule lives in a component or a handler.

Principles, enforced in review:

- **SOLID**: one reason to change per module; ports are small, role-specific traits; use cases depend on traits, never on adapters; adapters are swapped in tests with in-memory fakes.
- **KISS**: the simplest thing that meets this spec. One binary, one SQLite file, no message brokers, no plugin systems.
- **YAGNI**: nothing for a second user, a second engine kind or a future feature until this spec asks for it.
- **Clean code**: names from this spec's vocabulary (song, playlist, live, prompt), small functions, no dead code, errors as typed domain errors mapped to HTTP status codes at the edge.

Code layout: one crate, `maio`, with one module per layer. A layer uses only the layers above it; `tests/layers.rs` fails the build otherwise.

```
src/
  domain/      types and rules; no dependencies beyond serde
  usecases/    use cases and port traits; uses domain
  adapters/    sqlite, engines, text engines, ffmpeg, live, relay; implement the ports in usecases
  web/         Leptos SSR app, server functions, JSON API, axum server; wires everything
  main.rs      the maio binary
tests/         tests through the public API of each layer
```

## 4. Signing in

There is one account, created on first start from `MAIO_ADMIN_USER` and `MAIO_ADMIN_PASSWORD`; without them Maio refuses to start, so it is never open. That account can do everything. There are no roles, no user management and no sign-up.

- Passwords are 10 to 128 characters, stored as Argon2id hashes, never logged or returned. The password is changed in the web app (current and new password); changing it ends all other sessions.
- Signing in creates a server-side session: 32 random bytes in the cookie `maio_session` (`HttpOnly`, `SameSite=Strict`, `Path=/`, `Secure` when `secure_cookies = true`). A session lasts 12 hours, or 30 days with "keep me signed in".
- Any state-changing request authenticated by cookie must carry an `Origin` header matching the host Maio is served on, else `403 bad_origin`.
- Sign-in limits: 5 failed attempts per 15 minutes; beyond that `429` with `Retry-After`. A wrong username and a wrong password take the same time and get the same `401 bad_credentials`.
- **API token** for scripts and other programs: `MAIO_API_TOKEN` (at least 32 characters) in `.env`. A request with `Authorization: Bearer <token>` acts as you, with no session and no `Origin` check. Without the variable, the API answers only the session cookie. The token is compared in constant time and never logged.
- Live streams need no account: anyone with a live playlist's address can listen (section 9).

## 5. Songs

### Making a song

A song is asked for with a prompt and optional settings:

| Field | Type | Default | Rules |
|-------|------|---------|-------|
| `prompt` | string | none | what it sounds like, at most 500 characters |
| `mood` | string | none | one of the moods in config |
| `genre` | string | none | one of the genres in config |
| `energy` | string | none | `low`, `medium` or `high` |
| `seconds` | int | 180 | 15 to 600 |
| `tempo` | string or int | none | `slow`, `medium`, `fast`, or a BPM from 40 to 220 |
| `vocals` | bool | `false` | songs are instrumental unless asked |
| `lyrics` | string | none | only with `vocals`; `[verse]` and `[chorus]` sections |
| `vocal_language` | string | `[lyrics] default_language` | a language code such as `en` |
| `lyrics_by` | string | `[lyrics] default_engine` | `"ace-step"` or a text engine id; used when `lyrics` is empty |
| `engine` | string | automatic | a music engine id |
| `seed` | int | random | the same seed and settings make the same song |
| `allow_noncommercial` | bool | `false` | allows engines whose licence says `commercial: "no"` |

A request needs `prompt`, `mood`, or both. Maio turns mood, genre, energy and tempo into the engine prompt through a template, picks the first healthy engine in config order that can meet the request (unless `engine` names one; one that cannot meet it gets `422`), and starts a job.

- A job is `waiting`, `making` (with `progress` 0 to 1 and `eta_s`), `done` (with the new song's id), `failed` (with a reason) or `cancelled`. Jobs run one at a time per engine slot, in the order asked. You can cancel a job that is waiting or making.
- With `vocals` and no `lyrics`: the lyrics come from `lyrics_by`. A text engine is asked first (30 s limit); if it fails, answers empty or times out, ACE-Step writes them (`write_lyrics`), and the job notes the fallback.
- Licences: an engine marked `commercial: "no"` is used only with `allow_noncommercial`; otherwise `409 noncommercial_engine`, and the web app asks before resending. An engine marked `"limited"` is allowed, and its licence note travels with the job and the song.
- Rate limits (`[limits]`): `make` (default `20/10m`) and `pending_jobs` (default 4). Over a limit: `429 rate_limited` with `Retry-After`.

### What a song carries

Each song is stored as `songs/{id}.<ext>` in the storage format (`opus` by default; `flac`, `mp3`, `aac`), loudness-normalised to -16 LUFS on import, plus a row with:

| Field | Source |
|-------|--------|
| `title`, `title_source` | titles in layers, below |
| `prompt`, `mood`, `genre`, `energy`, `seconds`, `bpm`, `seed`, `vocals` | the request and the engine |
| `key`, `time_signature` | reported by the engine when it knows them |
| `lyrics`, `vocal_language` | the words actually sung |
| `description` | one line, when a text engine named the song |
| `engine`, `licence`, `lyrics_engine`, `title_engine` | credits and licence terms |
| `artist` | `"Maio"` |
| `created_at`, `bytes` | |

Titles, in layers; each applies only when the one above gives nothing:

1. **You** renamed it: `title_source = "person"`, never overwritten.
2. **A text engine**, when `[naming] engine` names one: it gets the structured fields, your prompt and the lyrics, and returns a title of at most 60 characters and a one-line description (15 s limit). `title_source = "engine"`.
3. **The lyrics**: the first line of the first `[chorus]`, else the first lyric line, in title case, at most 60 characters. `title_source = "lyrics"`.
4. **Word lists** per mood and genre, picked by the seed ("Low Tide Static"); a title the library already has moves to the next seed. `title_source = "words"`.

Files carry tags: title, artist, genre, BPM, lyrics, and a comment with the engine, seed and licence. A rename rewrites the tags without re-encoding.

## 6. Engine contract

An engine is an HTTP server that wraps one model and exposes the endpoints below. Local models and cloud APIs use the same contract, so Maio has one engine client and no engine-specific code. Wrappers live outside this repo; the ACE-Step one is the `maio-engine` repo, a small Python server (FastAPI).

Config:

```toml
[[engine]]
id = "ace-step"
url = "http://host.docker.internal:7801"   # ACE-Step on the Mac host
```

Engine order in config is the preference order for automatic choice (section 3). Engines take no token or password. An engine server binds `127.0.0.1`, so only this Mac and its containers reach it (Docker Desktop forwards `host.docker.internal` to the host's loopback). An engine on another machine has no protection of its own, so it belongs only on a network you trust.

### Endpoints

| Method | Path               | Notes |
|--------|--------------------|-------|
| GET    | `/info`            | identity, licence, capabilities |
| GET    | `/health`          | `200 {"status": "ok"}`, or `503 {"status": "loading"}` while the model loads |
| POST   | `/jobs`            | start a job; `202 {"job_id": "..."}`, or `429` when all slots are busy |
| GET    | `/jobs/{id}`       | job state |
| GET    | `/jobs/{id}/audio` | `audio/wav`, only when the job is `done` |
| DELETE | `/jobs/{id}`       | cancel a running job, or free a finished one |

### `GET /info`

```json
{
  "protocol": 1,
  "id": "ace-step",
  "name": "ACE-Step",
  "version": "1.5",
  "licence": {"name": "MIT", "commercial": "yes", "url": "https://...", "note": ""},
  "capabilities": {"min_seconds": 10, "max_seconds": 600, "vocals": true, "tempo": true, "concurrency": 1},
  "realtime_factor": 0.43
}
```

- `commercial` is `"yes"`, `"limited"` (for example Stable Audio Open's revenue cap; explain in `note`), or `"no"` (for example MusicGen).
- `realtime_factor` is generation time divided by audio length. It feeds `eta_s` and tells playout how early to start a job.
- Maio reads `/info` at startup and again whenever an engine comes back from `down`. If `protocol` is not one Maio knows, the engine is marked `incompatible` and never used.

### `POST /jobs`

```json
{"request": {"prompt": "calm ambient electronica, soft pads, 70 bpm, instrumental",
             "seconds": 180, "bpm": 70, "vocals": false, "lyrics": null, "seed": 1234,
             "vocal_language": "en", "write_lyrics": false}}
```

Maio has already resolved mood and tempo into `prompt` and `bpm`, and it never sends a request outside the declared capabilities. The engine still checks them and answers `422` if a request breaks them.

### `GET /jobs/{id}`

```json
{"job_id": "...", "state": "running", "progress": 0.4}
```

```json
{"job_id": "...", "state": "done", "seconds": 179.8, "seed": 1234, "lyrics": "...",
 "metas": {"bpm": 92, "key": "C major", "time_signature": "4"}}
```

`lyrics` and `metas` are optional; an engine reports them when it knows them.

```json
{"job_id": "...", "state": "failed", "error": {"code": "oom", "message": "...", "retryable": false}}
```

`state` is `queued`, `running`, `done`, `failed` or `cancelled`. Maio polls every 2 s while a job runs.

### Lifecycle

- When a job is `done`, Maio downloads the audio, then sends `DELETE /jobs/{id}`. The engine may drop finished jobs that nobody fetched after 1 hour.
- Maio checks `/health` every 30 s and marks an engine `down` after 3 failures. While an engine is `down`, Maio keeps checking and does not send it jobs.
- A connection error or a vanished job during a run counts as `retryable: true`. Maio retries once on the next healthy engine that can meet the request, then marks the queue entry `failed`.
- The engine never touches the library. Maio transcodes, normalises and stores the file.

A cloud engine is a small server that calls the vendor API and returns the downloaded audio from `/jobs/{id}/audio`. Its licence block records that vendor's terms.

## 7. Text engines

Lyrics, titles and descriptions come from text engines: language models that write words, never music. Like music engines they are listed in config.

| Kind | Talks to | Notes |
|------|----------|-------|
| `ace-step` | ACE-Step's planning model, inside the music job | lyrics only; available wherever an ACE-Step engine is |
| `anthropic` | the Anthropic Messages API | needs an API key |
| `openai` | any OpenAI-compatible chat API (llama.cpp, Ollama, LM Studio, vLLM) | key optional |

- A text engine has an `id`, a `kind`, a `model`, for the HTTP kinds a `url` and an optional `api_key_env`, and a `licence` for credits.
- Maio sends only your prompt, the structured fields and the lyrics, with fixed instructions. Reasoning text (`<think>…</think>`) in replies is dropped.
- Health: a one-token call at start and every 5 minutes.

## 8. Playlists and the player

- A playlist has an `id`, a `name`, its songs in order, and its live settings (section 9).
- You create, rename and delete playlists, add songs (at a position, default the end; songs already in it are skipped), remove songs, and set the whole order.
- The player runs in the browser. It plays one song or a playlist from any song, with play and pause, previous and next, a seek bar, volume, shuffle, and repeat (off, all, one). Songs stream from `GET /api/v1/songs/{id}/audio` with HTTP range requests. The player is per browser and changes nothing on the server.

## 9. Live

A playlist goes live with a switch. While live:

- Maio plays the playlist on the server, looping, with a fixed equal-power crossfade (default 6 s). With **shuffle** on (a setting of the live playlist) it plays every song once in random order, then reshuffles; with shuffle off it plays the playlist's order. A song added while live joins the loop; a removed song leaves it after it finishes playing.
- Everyone who tunes in hears the same song at the same moment. An empty live playlist streams silence.
- **Addresses** need no account and play in any player:
  - public: `/live/{playlist}/stream.mp3`, `/live/{playlist}/stream.aac`, `/live/{playlist}/hls/index.m3u8`;
  - private: `/listen/{key}/…` with the same files, where `key` is 32 random URL-safe characters; the public paths answer `404`. A new key stops the old one at once and disconnects its listeners. Switching between public and private also disconnects current listeners.
  - RTSP, SRT and RTMP through MediaMTX (`[relay]`) at `live/{playlist}` or `listen/{key}`; MediaMTX asks Maio at `POST /relay/auth` who may read or publish. Audio is AAC, since RTMP needs it and MediaMTX does not transcode.
- MP3 and AAC listeners that send `Icy-MetaData: 1` get `StreamTitle='Title - Maio'` every 16 000 bytes.
- The playlist page shows the live state, listener count (HTTP listeners plus relay readers), what plays now and next, and the addresses with copy buttons.
- Going live is refused beyond `max_live`; taking a playlist off air stops its encoders and disconnects its listeners.

## 10. HTTP API

**Everything the web app does, the API does too**: the web app has no private calls, so a script with the API token can make songs, build playlists, go live and change anything you can. All paths sit under `/api/v1` and need the session cookie or the API token; bodies are JSON; errors look like `{"error": {"code": "song_in_playlist", "message": "...", "playlists": [...]}}` with a matching status.

| Method | Path | Notes |
|--------|------|-------|
| POST | `/session` | `{"username","password","remember"}`; `204` and the cookie |
| DELETE | `/session` | sign out |
| POST | `/me/password` | `{"current","new"}` |
| GET | `/status` | engines (music and text) with health, live playlists, disk usage, moods and genres |
| GET | `/events` | server-sent events: `job`, `song`, `playlist`, `live` |
| POST | `/songs` | a song request (section 5); `202 {"job_id", "eta_s"}` |
| GET | `/jobs` | jobs waiting or making |
| DELETE | `/jobs/{id}` | cancel |
| GET | `/songs?q=&sort=newest\|title&limit=&cursor=` | search covers titles, prompts, moods, genres and lyrics; each song lists the playlists it is in |
| GET | `/songs/{id}` | one song |
| PATCH | `/songs/{id}` | `{"title"}`; `title_source = "person"`, tags rewritten |
| DELETE | `/songs/{id}` | only when in no playlist |
| GET | `/songs/{id}/audio` | the file, with range requests |
| GET | `/playlists` | all playlists with song counts, length and live state |
| POST | `/playlists` | `{"name"}` |
| GET | `/playlists/{id}` | the playlist and its songs in order |
| PATCH | `/playlists/{id}` | `{"name"}` |
| DELETE | `/playlists/{id}` | not while live |
| POST | `/playlists/{id}/songs` | `{"song_ids": [...], "position"?}` |
| PUT | `/playlists/{id}/songs` | `{"song_ids": [...]}`, the whole new order |
| DELETE | `/playlists/{id}/songs/{song_id}` | remove from this playlist only |
| PUT | `/playlists/{id}/live` | `{"live": bool, "shuffle"?: bool, "visibility"?: "public"\|"private"}` |
| GET | `/playlists/{id}/live` | live state, now playing, next, listeners, addresses |
| POST | `/playlists/{id}/live/key` | a new private key |

Stream and relay paths (`/live/…`, `/listen/…`, `/relay/auth`) sit outside `/api/v1` and need no session.

## 11. Config

```toml
listen = "0.0.0.0:8080"
data_dir = "/data"
secure_cookies = false
max_live = 4
crossfade_s = 6.0
moods = ["calm", "focused", "upbeat", "playful", "tense", "dreamy", "melancholy", "triumphant"]
genres = ["lo-fi", "ambient", "synthwave", "jazz", "orchestral", "rock", "electronic"]

[limits]
make = "20/10m"
pending_jobs = 4

[storage]
format = "opus"        # opus | flac | mp3 | aac
bitrate_k = 160

[serve]
mp3 = true
aac = true
hls = true

[relay]
enabled = true
publish = "rtsp://mediamtx:8554"
api = "http://mediamtx:9997"
host = "localhost"
rtsp_port = 8555
srt_port = 8891
rtmp_port = 1936

[[engine]]
id = "ace-step"
url = "http://host.docker.internal:7801"

# [[text_engine]]
# id = "local"
# kind = "openai"
# url = "http://host.docker.internal:8081/v1"
# model = "qwen3-8b"

# [naming]
# engine = "local"

[lyrics]
default_engine = "ace-step"
default_language = "en"
```

## 12. Web app

The design is the **Shelf** direction on the Claude Design canvas "Maio songs and playlists" (https://claude.ai/artifact/3ompMaRTUNVrNBvQLSYDS8, row C).

- **Look**: dark by default with a light theme; a sun/moon switch in the header, remembered on the device. Unbounded for headings, Albert Sans for text. Each playlist has its own colour (mustard `#E8B04A`, blue `#6FA8DC`, rose `#D9706C`, sage `#7FB89A`, then repeating), used for its shelf tile, its page band and the marks on songs. The player bar is mustard.
- **Icons over text** for actions (play, shuffle, add, more, live, lock, refresh, copy, cancel, remove), each with a tooltip and an accessible label.
- **Songs** screen:
  - the shelf: a sideways-scrolling row of playlist tiles, the "+" tile first, then live playlists, then the rest by most recently played;
  - "Make a song": a large prompt box and a Make button, settings as icon chips (mood, genre, length, tempo, seed) and a Vocals switch (instrumental by default); turning vocals on shows the lyrics box with language and lyrics writer;
  - "Being made": jobs with progress, time left and cancel;
  - the song list with search: play, title and prompt, playlist marks, mood and key, length, add to playlist, and rename or delete.
- **Playlist** page: a coloured band with the name and length and icon buttons (play, shuffle, add songs, more); the live strip under it (live switch with listener count, public or private, stream shuffle, new link, a copy button per address, streams in use); the songs, full width, with drag to reorder, play, the song on the live stream marked, and remove.
- **Player bar** on every screen: current song and playlist, shuffle, previous, play and pause, next, repeat, seek bar, volume.
- **Sign in** and **change password** screens.
- Every screen works from 360 px wide, with visible keyboard focus and text contrast of at least 4.5:1.

## 13. Logs, traces and metrics

- **Logs** go to stdout as text, or JSON with `log_format = "json"`; `RUST_LOG` filters them (default `info`). They name jobs, songs, playlists and engines by id; they never contain a password, the API token, a session id, an API key or a request header.
- **OpenTelemetry**: with `OTEL_EXPORTER_OTLP_ENDPOINT` set (the standard OpenTelemetry variables apply; `OTEL_SERVICE_NAME` defaults to `maio`), traces, metrics and logs also go out over OTLP/HTTP. Every API request is a span; each job's log lines carry its id.
- **Metrics**: `http.server.request.duration` by route, method and status; `maio.jobs` by outcome (`done`, `failed`, `cancelled`); `maio.engine.up` per music engine.
- On Ctrl-C or SIGTERM Maio stops taking requests and flushes what it has not exported yet.

## 14. Removed from v0

Radios and the radio dial, stations, the queue and its sides, asking for a song to play next on a radio, history and "Just played", favourites and Kept, the built-in Crate, Kept and Requests playlists, smart and generative playlists, users, roles and the admin flag, the storage budget and the retention sweep. The v0 database is not migrated.
