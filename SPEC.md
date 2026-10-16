# Maio spec (v0)

Maio makes music with local or cloud generation models and streams it as live radios.

Maio itself does no GPU work, so it runs in Docker with its own compose file and its own network. Each model runs as its own engine server wherever the hardware is: ACE-Step natively on the Mac host (Docker on a Mac cannot reach the GPU), YuE on an NVIDIA machine, cloud services anywhere. Maio talks to every engine through the same HTTP contract.

This spec covers two contracts: the HTTP API and the engine contract. Everything else is sketched only as far as those contracts need.

## 1. Shape

```
 Docker (Maio's own compose and network)           Mac host / other machines
 ┌────────────────────────────────────────────┐
 │ web UI ──┐                                 │
 │ apps ────┼── HTTP API ── maio (Rust)  ──┼──HTTP──> ace-step engine (Mac, Metal)
 │ client ──┘                 │              ├──HTTP──> yue engine (NVIDIA box)
 │                            ├── library     ├──HTTP──> cloud engine (vendor API)
 │                            ├── playout     │
 │                            └── ffmpeg ─────┼──> mediamtx, Icecast, HLS
 └────────────────────────────────────────────┘
```

- Maio is one container: one binary plus ffmpeg, one config file (`maio.toml`), one data volume.
- Engine servers are separate programs that Maio does not start or supervise. On the Mac, launchd keeps the ACE-Step server running.
- Playout produces one PCM stream (48 kHz, stereo, s16). ffmpeg takes it on stdin and handles every output.

## 2. Auth and scopes

Maio shares no Docker network with other apps. It publishes port 8080 on the host, and everything reaches it through that port: the web UI at `http://localhost:8080`, and listeners' players at the stream URLs. Every `/api/v1` request needs a person signed in through the web UI (a session cookie, section 8). No app or machine reaches the API.

| Scope     | Allows                                                        |
|-----------|---------------------------------------------------------------|
| `read`    | radios, status, queue, library, history, events                |
| `queue`   | add any request or library track, remove queue entries, edit stations |
| `control` | skip, favourite, switch the active station                    |
| `admin`   | start and stop playout, use non-commercial engines            |

People get scopes from their role (section 8).

**No machine access for now.** Maio is radio stations: people run them in the web UI, and anyone with a radio's listen address can listen (section 7). There are no machine tokens, and no app, bot or producer can call the API.

Maio runs like a station: it keeps playing whether or not anyone is listening, and other listeners may share the same stream. So no listener stops Maio: a listener who wants no more music closes the stream.

### Rate limits

Every signed-in person has limits, set in `[limits]` in `maio.toml`. Generation costs GPU time, so the limits mostly guard that.

```toml
[limits]
queue = "20/10m"
pending_jobs = 4
```

| Limit          | Counts                                                        | Default |
|----------------|---------------------------------------------------------------|---------|
| `queue`        | `POST /queue` calls                                           | `20/10m` |
| `generate`     | `POST /generate` calls                                        | `20/10m` |
| `station`      | `PUT /station` and on-air changes                             | `10/1h` |
| `write`        | every other call that changes state                           | `60/1m` |
| `pending_jobs` | generation jobs from this person that are not yet done         | 4 |

- A call over a limit gets `429 rate_limited` with a `Retry-After` header. A request beyond `pending_jobs` gets the same answer, with no retry time.
- Reads are not limited.
- Station autofill is Maio's own work and counts against no one.
- Counters live in memory, so a restart resets them.

No scope can change the volume. The output level is fixed in config, and every track is loudness-normalised when it enters the library (see 6).

## 3. HTTP API

All paths sit under `/api/v1`, and bodies are JSON. Errors look like `{"error": {"code": "engine_unavailable", "message": "..."}}` with a matching HTTP status.

### Status and events

| Method | Path       | Scope  | Notes |
|--------|------------|--------|-------|
| GET    | `/status`  | read   | now playing, position, queue, playout state, engines, outputs |
| GET    | `/events`  | read   | Server-sent events: `now_playing`, `queue`, `job`, `station`, `output`. Works with the session cookie |

`now_playing` event:

```json
{"track": {"id": "trk_01J...", "title": "calm, 92 bpm", "engine": "ace-step", "seconds": 184},
 "started_at": "2026-10-15T20:31:04Z", "position_s": 12.4}
```

### Queue

| Method | Path                 | Scope            | Notes |
|--------|----------------------|------------------|-------|
| GET    | `/queue`             | read             | entries in play order |
| POST   | `/queue`             | queue or request | add an entry |
| DELETE | `/queue/{entry_id}`  | queue            | remove an entry that has not started |
| DELETE | `/queue?side=a\|b`   | queue (+control for b) | `side=b` clears the station's tracks and turns the station off. Default `side=a`: clear side A: every queued entry that a person added and that has not started; their jobs are cancelled. Station tracks and library fillers stay; stopping the station ends those |
| POST   | `/skip`              | control          | crossfade to the next entry now |

`POST /queue` body, one of:

```json
{"track_id": "trk_01J...", "position": "end"}
```

```json
{"request": {"mood": "calm", "seconds": 180, "tempo": "slow"}, "position": "next"}
```

`position` is `"next"` (directly after the current track) or `"end"` (the default).

Response `202`:

```json
{"entry_id": "q_01J...", "state": "generating", "job_id": "job_01J...", "eta_s": 80}
```

A queue entry is in one of these states: `generating`, `ready`, `playing`, `played`, `failed`, `replaced`.

### Generation without queueing

| Method | Path          | Scope | Notes |
|--------|---------------|-------|-------|
| POST   | `/generate`   | queue | body is a `request`; the result lands in the library only |
| GET    | `/jobs/{id}`  | read  | `pending`, `running` (with `progress` 0..1), `done` (with `track_id`), `failed` |

### Library and history

| Method | Path                         | Scope   | Notes |
|--------|------------------------------|---------|-------|
| GET    | `/library?q=&mood=&fav=&side=&station=&limit=&cursor=` | read | `side=a`: requests (no station); `side=b`: made by a station; `station=<id>`: one station's tracks |
| GET    | `/library/{id}`              | read    | metadata |
| DELETE | `/library/{id}`              | queue   | delete the track and its file; `409 track_in_use` when queued or playing |
| DELETE | `/library`                   | admin   | empty the crate: delete every track that is not kept, queued or playing; returns `{"deleted": n}` |
| GET    | `/library/{id}/audio`        | read    | FLAC file |
| PUT    | `/library/{id}/favourite`    | control | body `{"favourite": true}` |
| GET    | `/history?limit=&cursor=`    | read    | plays, newest first |

### Stations

*Replaced by playlists in stage 3 (section 10). This is the stage 1 and 2 behaviour.*

A station is a standing request that keeps the stream going. Whenever the queue runs short, playout asks the active station for the next track, and each track is a new generation in the station's style. Entries queued by hand always play first; the station only fills the gaps.

| Method | Path               | Scope   | Notes |
|--------|--------------------|---------|-------|
| GET    | `/stations`        | read    | |
| POST   | `/stations`        | queue   | create |
| PUT    | `/stations/{id}`   | queue   | replace |
| DELETE | `/stations/{id}`   | queue   | cannot delete the active station |
| GET    | `/station`         | read    | the active station, or `null` |
| PUT    | `/station`         | control | body `{"id": "st_01J..."}`; `{"id": null}` turns stations off and needs `control` |

On a switch, queued entries that the old station made and that have not started become `replaced`, and their jobs are cancelled. Entries queued by hand stay.

Station object:

```json
{
  "id": "st_01J...",
  "name": "late night coding",
  "prompt": "lo-fi hip hop, warm vinyl, mellow keys",
  "mood": "calm",
  "seconds": [150, 240],
  "tempo": "slow",
  "variations": ["with rain ambience", "jazzy chords", "soft bass"],
  "library_share": 0.25,
  "engine": null
}
```

| Field           | Rules |
|-----------------|-------|
| `prompt`, `mood`, `genre`, `energy`, `tempo`, `engine` | as in the request object; needs `prompt`, `mood`, or both |
| `seconds`       | `[min, max]`, each 15 to 600; each track gets a random length in the range |
| `variations`    | optional; each track appends one, picked at random, to the prompt |
| `library_share` | 0 to 1, default 0.25; the share of gaps filled with library tracks from this station instead of new ones |

Station tracks are always instrumental and always use a random seed. A station cannot use a non-commercial engine. The active station survives a restart, and its id shows in `GET /status`.

### Engines and playout

| Method | Path              | Scope | Notes |
|--------|-------------------|-------|-------|
| GET    | `/engines`        | read  | id, licence, capabilities, health |
| POST   | `/playout/start`  | admin | |
| POST   | `/playout/stop`   | admin | fades out, then outputs send silence so listeners stay connected |

### The request object

Every way of asking for music uses this object.

| Field      | Type   | Default  | Rules |
|------------|--------|----------|-------|
| `mood`     | string | none     | one of the moods in config (see 5) |
| `genre`    | string | none     | one of the genres in config (see 5) |
| `energy`   | string | none     | `low`, `medium` or `high` |
| `prompt`   | string | none     | free text, at most 500 chars; needs scope `queue` |
| `seconds`  | int    | 180      | 15 to 600 |
| `tempo`    | string or int | none | `slow`, `medium`, `fast`, or a BPM from 40 to 220 |
| `vocals`   | bool   | `false`  | `true` needs an engine that can sing, plus scope `queue` |
| `lyrics`   | string | none     | only when `vocals` is true |
| `engine`   | string | auto     | needs scope `queue` |
| `seed`     | int    | random   | |
| `allow_noncommercial` | bool | `false` | needs scope `admin` |

A request needs `mood`, `prompt`, or both. The server turns mood, genre, energy and tempo into a prompt through a template for each engine, then hands the engine the normalised request (section 4).

Engine choice: if `engine` is missing, the server takes the first healthy engine in config order that can meet the request. A request that asks for a specific engine that cannot meet it gets `422`.

Licences: an engine whose licence says `commercial: "no"` is used only when `allow_noncommercial` is true. Without that flag the server returns `409 noncommercial_engine`, and the web UI shows a confirm dialog and resends with the flag. An engine marked `commercial: "limited"` is allowed, but its warning travels on the job and the track.

## 4. Engine contract

An engine is an HTTP server that wraps one model and exposes the endpoints below. Local models and cloud APIs use the same contract, so Maio has one engine client and no engine-specific code. The repo ships each wrapper as a small Python server (FastAPI) under `engines/`.

Config:

```toml
[[engine]]
id = "ace-step"
url = "http://host.docker.internal:7801"   # ACE-Step on the Mac host
token_env = "MAIO_ACE_STEP_TOKEN"

[[engine]]
id = "yue"
url = "http://gpu-box.lan:7802"
token_env = "MAIO_YUE_TOKEN"
```

Engine order in config is the preference order for automatic choice (section 3). Every engine request carries `Authorization: Bearer <token>`. An engine server that only Maio on the same Mac uses binds `127.0.0.1`, because Docker Desktop forwards `host.docker.internal` to the host's loopback.

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

## 5. Moods and genres

Both lists live in config, and `GET /status` returns them (`moods` and `genres` fields) so the web UI and the server agree.

- Moods: `calm`, `focused`, `upbeat`, `playful`, `tense`, `dreamy`, `melancholy`, `triumphant`.
- Genres: `lo-fi`, `ambient`, `synthwave`, `jazz`, `orchestral`, `rock`, `electronic`.

`energy` is fixed: `low`, `medium`, `high`.

## 6. Library and playout (sketch)

- *Stage 3 replaces this rule with layered titles (section 10).* Each track gets a title built from structured fields: the station name if a station made it, otherwise mood, genre and tempo (for example `late night coding` or `tense synthwave, fast`). A track with only a prompt gets `untitled`. The web UI can rename a track; a rename is owner text, like a station name.
- Each track is stored as `tracks/{id}.<ext>` in the configured storage format (below), loudness-normalised to -16 LUFS on import with ffmpeg `loudnorm`. The SQLite row holds prompt, mood, genre, energy, request, engine, licence, seconds, bpm, vocals, seed, favourite, requester token name, and created_at.
- The history table records track_id, started_at, ended_at and whether the track was skipped.
- Playout keeps `lookahead` entries ready (default 2) and starts each job early enough, based on `realtime_factor` plus 30 s of slack for download and import, to finish before the entry is needed. If the queue runs short, it asks the active station for a new track (section 3, Stations); when an engine fails, the station fills from the library: its own tracks first, then favourites, never repeating a track from the last `no_repeat` plays (default 20). With no active station and nothing queued, the stream is silent. A requested track that is late gets a library stand-in with its mood (or any favourite if none match) and stays at the head of the queue; if its job fails, it becomes `failed` and the queue moves on.
- Storage: `[storage]` sets `format` (`opus` default, `flac`, `mp3`, `aac`), `bitrate_k` for the lossy formats (default 160), and `max_gb`, a budget for the crate (default 20; 0 means none). Opus at 160 kbit/s is about 1.2 MB a minute, against about 10 MB for FLAC.
  - Before every generation and after every import, Maio deletes tracks until the new one fits: least recently played first (a never-played track counts from when it was made), never kept, queued or playing tracks.
  - When only kept tracks are left, the crate is full: new requests and `POST /generate` answer `507 library_full`, while picking an existing track still works. A station stops generating and replays its own tracks. `GET /status` reports `storage: {format, used_bytes, max_bytes, full}`, and the web UI shows the usage and a notice.
- Retention: a station running all day makes about 20 tracks an hour, roughly 600 MB as FLAC or 70 MB as Opus. A daily sweep deletes tracks that are not favourites, are not queued, and are older than `retention_days` (default 7, `0` keeps everything). History rows keep their metadata after the file is gone.
- Crossfade: a fixed equal-power fade, default 6 s. Skip uses the same fade.

## 7. Outputs (sketch)

```toml
[[output]]            # push
kind = "rtsp"         # rtsp | srt | rtmp | icecast
url = "rtsp://host.docker.internal:8554/music"   # a MediaMTX on the host, through its published port
codec = "opus"        # opus | aac | mp3

[serve]               # pull
mp3 = true            # GET /stream.mp3, Icecast-style, served by axum
aac = true            # GET /stream.aac, raw AAC in ADTS frames, served by axum
hls = true            # GET /hls/index.m3u8, segments written by ffmpeg
```

Every radio has a listen address that needs no account and plays in any player. A public radio's address is its name: `/radios/{radio}/stream.mp3`, `.aac` and `hls/index.m3u8`. A private radio's address carries a secret key instead: `/listen/{key}/stream.mp3`, `.aac` and `hls/index.m3u8`, where `key` is 32 random URL-safe characters; its `/radios/{radio}/…` paths answer `404`. `/api/v1` always needs someone signed in. Both MP3 and ADTS frames carry their own headers, so a listener can join at any moment. The web UI's player offers the formats the server serves; Auto picks AAC when the browser can play it, else MP3.

Every radio can also be pulled over RTSP, SRT and RTMP from a MediaMTX service in Maio's own compose (`[relay]`). Maio publishes each radio to it as AAC at `radios/<id>` (public) or `listen/<key>` (private), and MediaMTX asks Maio at `POST /relay/auth` (its HTTP auth hook) who may do what: only Maio publishes and uses the control API, and anyone may read a path that is a current radio's. Through MediaMTX's control API Maio counts those readers into the radio's listeners and disconnects readers of the old path when a radio's key or visibility changes. The addresses (`rtsp://host:port/<path>`, `srt://host:port?streamid=read:<path>`, `rtmp://host:port/<path>`) are listed with the radio. WebRTC is out: it needs Opus, RTMP needs AAC, and MediaMTX does not transcode. Serving Opus over HTTP is out too, because a listener who joins an Ogg stream midway needs the stream headers replayed; for Opus, push to a real Icecast server.

## 8. Users and sign-in

People sign in to the web UI with a username and password.

### Users and roles

A user has a `username` (2 to 32 characters from `a-z0-9._-`, unique), a display `name`, a `role`, a password hash, a `must_change_password` flag, `created_at` and `last_seen_at`. Users live in the SQLite store.

| Role       | Scopes                                         | For |
|------------|------------------------------------------------|-----|
| `admin`    | all                                            | users, playout start and stop, non-commercial engines, and everything below |
| `operator` | `read`, `queue`, `control`                     | running the music |
| `listener` | `read`                                         | seeing what plays and listening |

- Passwords are 10 to 128 characters and are stored as Argon2id hashes. Plain passwords are never logged or returned.
- On first start with no users, Maio creates an admin from `MAIO_ADMIN_USER` and `MAIO_ADMIN_PASSWORD`. Without them it refuses to start, so a fresh install is never open.
- An admin can't remove or demote the last admin, and can't remove themselves.
- A password set by an admin (new user or reset) is temporary: `must_change_password` is set, and the user must choose a new one before any other call succeeds (`403 password_change_required`).

### Sessions

- Signing in creates a server-side session: 32 random bytes, sent as the cookie `maio_session` with `HttpOnly`, `SameSite=Strict` and `Path=/`, plus `Secure` when `secure_cookies = true` in config (set it behind HTTPS).
- A session lasts 12 hours, or 30 days with "keep me signed in", and ends at sign-out, at a password change (all of that user's sessions), and when the user is removed.
- Cross-site protection: besides `SameSite=Strict`, any state-changing request authenticated by cookie must carry an `Origin` header that matches the host Maio is served on, else `403 bad_origin`.

### Sign-in limits

- Per username: 5 failed attempts per 15 minutes. Per client address: 20 per 15 minutes. Beyond either, `429` with `Retry-After`.
- An unknown username takes the same time as a wrong password (a dummy hash is checked), and both get the same `401 bad_credentials`.

### Endpoints

| Method | Path                               | Who      | Notes |
|--------|------------------------------------|----------|-------|
| POST   | `/session`                         | anyone   | `{"username","password","remember"}`; `204` and the cookie, or `401 bad_credentials` |
| DELETE | `/session`                         | signed in | sign out |
| GET    | `/me`                              | signed in | `{"username","name","role","must_change_password"}` |
| POST   | `/me/password`                     | signed in | `{"current","new"}`; ends the user's other sessions |
| GET    | `/users`                           | admin    | list, without hashes |
| POST   | `/users`                           | admin    | `{"name","username","password","role"}`; the password is temporary |
| PATCH  | `/users/{username}`                | admin    | `{"name"?, "role"?}` |
| POST   | `/users/{username}/password`       | admin    | `{"password"}`; temporary; ends that user's sessions |
| DELETE | `/users/{username}`                | admin    | not yourself, not the last admin |

All paths sit under `/api/v1`. Sign-in calls and user changes are written to the log with the acting user, never with passwords or secrets.

### Web UI

- `/` serves the app; without a valid session it shows the sign-in screen. The design lives in the Claude Design canvas "Maio web admin" (Vinyl and tape theme: paper `#F3EAD8` / label red `#C2301A` light, sleeve `#17120E` / `#F0603F` dark; Big Shoulders Display, Barlow, Courier Prime).
- Light or dark follows the system, with a manual switch remembered on the device.
- The player strip plays `/stream.mp3` in the page. Its volume changes only this device, never the station.
- Up next shows side A (queued, with "Clear side A") and side B (station tracks and library fillers).

## 10. Playlists, song metadata and lyrics (stage 3)

Stage 3 replaces stations with playlists: everything that can feed a stream is a playlist. It runs several radios at once, so different people can listen to different radios. It also gives every song real metadata (a title, an artist, lyrics) inside Maio, in the files, and in the streams.

### Radios

A radio is one live channel. Each radio has its own:

- side A and side B queue (section 3), playout loop and crossfade;
- on-air playlist (or none), with silence when idle;
- listen address: `/radios/{radio}/…` when public, `/listen/{key}/…` when private (MP3, AAC and HLS, section 7);
- push outputs: each `[[output]]` names its `radio` (for example one MediaMTX path per radio);
- now playing, history and listener count.

Playlists, the crate and users are shared by all radios. The same playlist can be on air on two radios at once; each radio shuffles and loops it independently.

- **Public or private:** each radio has a `visibility`, `public` (the default) or `private`, and a listen address (section 7). A public radio's address uses its name, so anyone who knows the name can listen. A private radio's address uses a secret key, so only people given the link can listen. Admins switch visibility and can make a new private link (`POST /radios/{radio}/listen-key`), which stops the old one at once and drops its listeners. Push outputs carry either kind. The web UI shows each radio's address with a copy button.
- **Creating radios:** admins create, rename and remove radios; a radio needs a unique `id` (`a-z0-9-`, 2 to 32 characters) and a `name`. One radio, `main`, exists from the start and cannot be removed; the old unprefixed paths (`/stream.mp3`, `/stream.aac`, `/hls/...`) are aliases for it.
- **Limits:** `max_radios` in config (default 4). Every radio runs its own encoders, so each costs some CPU. All radios share the engines: generation jobs from every radio wait in one line, side A requests first, then on-air playlists in turn, so two generative radios share ACE-Step's time.
- **Who listens to what:** everyone signed in picks a radio in the web UI; the choice is remembered on the device, and the player, queue, now playing and controls follow it. The radio list shows each radio's on-air playlist, its current song and how many people are listening (open stream connections, counted per radio).
- **Starting and stopping:** a radio can be put off air on its own (`POST /radios/{radio}/stop`, admin), silencing only that radio's streams.

### Everything is a playlist

A playlist has an `id`, a `name`, a `kind`, an `order` (`in_order` or `shuffle`), a `max_tracks`, and its tracks. A track can be in many playlists.

- **Size limit:** `max_tracks` defaults to `[playlists] max_tracks` (50) and can be set per playlist up to `[playlists] max_tracks_limit` (500). Adding tracks to a full fixed playlist answers `409 playlist_full`. A generative playlist that reaches its size stops making new tracks and loops the ones it has, like a fixed playlist; raising `max_tracks` or removing tracks lets it make new ones again. Smart playlists play at most `max_tracks` of their matches, newest first. Built-ins have no limit.

| Kind | Tracks come from | Notes |
|------|------------------|-------|
| `crate` | every live track | built in; one per install; cannot be deleted or edited |
| `kept` | tracks with `favourite` set | built in; the heart button adds and removes |
| `requests` | tracks made from one-time requests (side A) | built in |
| `fixed` | tracks people add by hand, in their order | |
| `generative` | a recipe; new tracks join it as they are made, and people can add or remove tracks by hand | today's stations |
| `smart` | a rule over the crate, evaluated on every read | no stored membership |

- **Recipe** (generative only): the station fields from section 3 (`prompt`, `mood`, `genre`, `energy`, `tempo`, `seconds`, `variations`, `library_share`, `engine`), plus `vocals` (default `false`), `vocal_language` (default `"en"`) and `lyrics_by` (`"ace-step"` or the id of a text engine, below; default from `[lyrics] default_engine`).
- **Rule** (smart only): any of `mood`, `genre`, `kept`, `side`, `from_playlist`, `max_seconds`, `made_within_days`. All given fields must match.
- **Who does what:** people (`queue` scope) create, edit and fill playlists. Maio fills generative playlists through the engine while they are on air, and evaluates smart ones.
- Removing a track from a playlist does not delete it. Deleting a track (from the crate) removes it from every playlist.
- Existing stations become generative playlists with the same ids, and each track's `station_id` becomes membership of that playlist. The `/stations` and `/station` endpoints go away; nothing outside Maio uses them yet.

### On air

- On each radio, exactly one playlist, or none, is on air. It feeds that radio's side B; side A (one-time requests) still plays first. With none on air and side A empty, the stream is silent (section 6).
- `fixed`, `smart` and built-in playlists play their tracks in their order, or shuffled, and loop; a track is not repeated within the last `no_repeat` plays unless the playlist is too short.
- A `generative` playlist mixes new tracks and its own older ones, using `library_share` as the share of replays. On a full crate (section 6, Storage), or once it reaches `max_tracks`, it only replays. *(Default chosen while this spec was written; say if you want a fallback playlist instead.)*
- "Clear side B" (section 3) takes the playlist off air.

### Storage protection

The storage budget (section 6) never deletes a track that is in `kept` or in any `fixed` playlist. Membership of `requests`, a `generative` or a `smart` playlist does not protect a track. *(Default chosen while this spec was written.)*

### Song metadata

Every track carries:

| Field | Source |
|-------|--------|
| `title`, `title_source` | layered naming, below |
| `artist` | the generative playlist that made it, else `"Maio"` |
| `album` | the generative playlist that made it, else `"Requests"` |
| `genre`, `mood`, `energy`, `bpm`, `seconds`, `seed`, `prompt`, `engine`, `licence` | as today |
| `key`, `time_signature` | reported by the engine when it knows them (ACE-Step does when its planning model runs) |
| `lyrics`, `vocal_language` | section below |
| `description` | one line, only when a text engine names the song |
| `lyrics_engine`, `title_engine` | which engine wrote the lyrics and the title, for credits and licence terms |

The record cover stays drawn from the seed.

### Titles, in layers

Each layer applies only when the one above gives nothing:

1. **A person** renamed it (`PATCH /library/{id}` with `title`): `title_source = "person"`. Never overwritten.
2. **A text engine**, when `[naming] engine` names one: Maio sends it the structured fields, the recipe prompt and the lyrics, and stores the title it returns (at most 60 characters) and a one-line description. Only text that people wrote reaches it. On error or after 15 seconds, the next layers apply. `title_source = "engine"`, and the track records which engine named it.
3. **The lyrics**, for vocal songs: the first line of the first `[chorus]` section, else the first lyric line, trimmed to 60 characters in title case. `title_source = "lyrics"`.
4. **Word lists**, for everything else: words chosen per mood and genre and picked by the seed ("Low Tide Static", "Midnight Rhodes"). If the crate already has that title, the next seed is tried. `title_source = "words"`.

Titles come from people's recipes and lyrics or from these word lists, never from outside callers.

### Tags in the files and the streams

- On import, the file gets tags: title, artist, album, genre, BPM, lyrics, and a comment with the engine, seed and licence (Vorbis comments for Opus and FLAC, ID3 for MP3, MP4 tags for AAC). A rename rewrites the tags without re-encoding.
- `/stream.mp3` and `/stream.aac` send in-stream titles to players that ask for them (`Icy-MetaData: 1`): `StreamTitle='Title - Artist'` every 16 000 bytes (`icy-metaint`). Players that do not ask get the plain stream as today. HLS and push outputs carry no titles in this stage.

### Lyrics

- The request object gains `vocal_language` (default `"en"`) and `lyrics_by` (needs scope `queue`). With `vocals: true`, `lyrics` stays optional: when missing, the lyrics are written for the song by `lyrics_by`: the request's, else the playlist's, else `[lyrics] default_engine`.
- Engine contract: the request gains `vocal_language` and `write_lyrics` (true when Maio has no lyrics and `lyrics_by` is `"ace-step"`). A done job may report `lyrics` (the text actually sung) and `metas` (`bpm`, `key`, `time_signature`). The ACE-Step server loads its planning model the first time a job needs it, so instrumental-only setups never pay for it.
- With `lyrics_by` naming a text engine, Maio asks it for lyrics (with `[verse]` and `[chorus]` sections, in `vocal_language`) from the recipe before it submits the job, and sends them as `lyrics`. If the text engine fails or takes over 30 seconds, ACE-Step writes them instead (`write_lyrics`), and the job notes the fallback.
- Lyrics are stored on the track, written into the file tags, and shown in the now-playing panel as plain text; there is no timing, so no karaoke scrolling.
- Section 3's "station tracks are always instrumental" no longer holds: each generative playlist chooses. Instrumental stays the default.

### Text engines

Lyrics, titles and descriptions come from text engines: language models that write words, never music. Like music engines they are listed in config, and any number can be set up. Maio ships three kinds:

| Kind | Talks to | Notes |
|------|----------|-------|
| `ace-step` | ACE-Step's planning model, inside the music job | lyrics only; always available when an ACE-Step engine is |
| `anthropic` | Claude through the Anthropic Messages API | needs an API key |
| `openai` | any OpenAI-compatible chat API: llama.cpp's `llama-server`, Ollama, LM Studio, vLLM | runs locally or anywhere; key optional |

- A text engine has an `id`, a `kind`, a `model`, and for the HTTP kinds a `url` and an optional `api_key_env`. It records its `licence` like a music engine does, so a track's credits name who wrote its words.
- Maio sends only text that people wrote (recipes, request prompts) and the structured fields. The prompts it uses are fixed in Maio and ask for plain lyrics with section tags, or for a title and one line of description.
- `GET /engines` lists text engines next to music engines, with their health (an `openai` or `anthropic` engine is checked with a one-token call at start and every 5 minutes).

### API

All under `/api/v1`. Everything that belongs to a radio moves under `/radios/{radio}`: `status`, `events`, `queue` (including clearing sides), `skip`, `history`, `on-air`, `start` and `stop`. The old unprefixed paths act on `main`. A radio-bound token always acts on its own radio, whatever the path says.

| Method | Path | Scope | Notes |
|--------|------|-------|-------|
| GET | `/radios` | read | id, name, `visibility`, on-air playlist, now playing, listener count, listen addresses |
| GET | `/radios/{radio}` | read | one radio, same fields |
| POST | `/radios` | admin | `{"id", "name", "visibility"?, "playlist_id"?}`; refused beyond `max_radios` |
| PATCH | `/radios/{radio}` | admin | `{"name"?, "visibility"?}` |
| POST | `/radios/{radio}/listen-key` | admin | private radios: a new secret listen address; the old one stops working |
| DELETE | `/radios/{radio}` | admin | not `main`; its queue is dropped and its outputs stop |
| GET | `/playlists` | read | all playlists with counts; built-ins first |
| POST | `/playlists` | queue | `{"name", "kind": "fixed"\|"generative"\|"smart", "order"?, "recipe"?, "rule"?}` |
| GET | `/playlists/{id}?limit=&cursor=` | read | the playlist and a page of its tracks |
| PATCH | `/playlists/{id}` | queue | name, order, recipe or rule; not built-ins |
| DELETE | `/playlists/{id}` | queue | not built-ins, not while on air; its tracks stay in the crate |
| POST | `/playlists/{id}/tracks` | queue | `{"track_ids": [...], "position"?}`; fixed and generative only |
| PUT | `/playlists/{id}/tracks` | queue | `{"track_ids": [...]}`, the whole new order |
| DELETE | `/playlists/{id}/tracks/{track_id}` | queue | remove from this playlist only |
| GET | `/radios/{radio}/on-air` | read | the playlist on air there, or `null` |
| PUT | `/radios/{radio}/on-air` | control | `{"playlist_id": "..."}` or `{"playlist_id": null}` |
| PATCH | `/library/{id}` | queue | `{"title"}`; sets `title_source = "person"` and rewrites the tags |
| GET | `/library?playlist=<id>` | read | the crate filtered to one playlist; replaces `station=` |

Events are per radio: `station` becomes `on_air`, and `now_playing` gains `artist`, `album` and `lyrics`. `GET /radios` changes are sent as `radios` events on every radio's stream.

### Config

```toml
max_radios = 4         # radios in total

[playlists]
max_tracks = 50        # default size of a new playlist
max_tracks_limit = 500 # the largest size anyone can set

[[output]]
radio = "lobby"                    # which radio this output carries; default "main"
kind = "rtsp"
url = "rtsp://host.docker.internal:8554/music"
codec = "opus"

[[text_engine]]
id = "claude"
kind = "anthropic"
model = "claude-haiku-4-5"
api_key_env = "ANTHROPIC_API_KEY"

[[text_engine]]
id = "local"
kind = "openai"                    # llama.cpp llama-server, Ollama, LM Studio, vLLM
url = "http://host.docker.internal:8081/v1"
model = "qwen3-8b"

[naming]
engine = "local"                   # a text engine id; leave out for lyrics and word lists only

[lyrics]
default_engine = "ace-step"        # "ace-step" or a text engine id
default_language = "en"
```

### Web UI

A radio picker sits in the header: the list of radios with their current song and listener count; picking one switches the player, queue, now playing and controls to it, and the choice is remembered on the device. Admins can add and remove radios there.

Anyone, listeners included, can open the playlist on air on any radio, not only the one they listen to: its tracks in play order, the one playing now marked, and the next few it will play (for shuffled playlists, the order already drawn). `GET /radios/{radio}/on-air` returns the playlist with that order and position.

A Playlists view replaces Stations: built-ins first (Crate, Kept, Requests), then people's playlists as sleeves, generative ones with their recipe and a "makes new tracks" mark. Each playlist page shows its tracks with add, remove and drag to reorder, and a button to put it on air. Crate rows get "add to playlist", rename, and a preview button that plays the track in this browser only (`/library/{id}/audio`), off air. Now playing shows the artist and the lyrics. The design goes on the canvas before it is built.

## 9. Stages

1. Done on branch `stage-1`: the ACE-Step engine server on the Mac (with a launchd plist), and the Maio container with library, playout, stations, push and serve outputs, and a token-only web UI.
2. Done on branch `stage-1`: users and sign-in (section 8), clearing sides A and B, the web UI from the design canvas, the AAC stream, track deletion, and the storage format and budget (section 6).
3. Radios, playlists, song metadata and lyrics (section 10). Radios, playlists, song metadata, lyrics and the Dial web UI are done.
4. A YuE engine server on an NVIDIA machine, Stable Audio Open for short pieces only (it declares `max_seconds: 47`, so stations and full-length requests never pick it; Maio does not chain clips), cloud engines, crossfade options, music that follows the scene.
