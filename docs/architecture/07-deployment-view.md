# 7. Deployment view

## 7.1 Nodes

```c4
deployment
```

- The compose project is `maio`, on its own network `maio_default`.
- `compose.yaml` runs the published image, `ghcr.io/fulgas/maio`, at the release `MAIO_VERSION` in `.env` names, or the latest. `compose.build.yaml`, which `just stack` adds, builds the image from the checkout instead, as `maio:dev`.
- The compose project runs on any Docker host. ACE-Step runs as its own server on the machine with the GPU (CUDA, Apple silicon, AMD ROCm, Intel XPU or CPU), set up as [ACE-Step's own instructions](https://github.com/ace-step/ACE-Step-1.5#readme) say. On the same machine, the `[[engine]]` url is `http://host.docker.internal:7801`; on another, it is that machine's address.
- `just` starts Maio and MediaMTX in Docker; ACE-Step runs on its own, as [Run ACE-Step for Maio](../how-to/01-run-ace-step-for-maio.md) says. `just fake` runs a fake engine (sine tones, no GPU) instead.
- The host ports 8555, 8891 and 1936 leave 8554, 8890 and 1935 free for another MediaMTX on the same machine. From another container, use `host.docker.internal:8555`; inside Maio's compose network, `mediamtx:8554`.
- MediaMTX asks Maio at `http://maio:8080/relay/auth` about every publish, read and API call.
- The image is built in two stages: `cargo leptos build --release` (server and WebAssembly, hashed asset names), then a slim runtime with ffmpeg.
- Each release also publishes this image as `ghcr.io/fulgas/maio` for `linux/amd64` and `linux/arm64`, and attaches `maio` with its web app for Linux and for Macs with Apple silicon to the GitHub Release ([ADR 0019](../adr/0019-releases-come-through-a-release-pull-request.md)).

## 7.2 The data volume

| Path | What |
|------|------|
| `library.db` | SQLite: songs, playlists, the account and sessions |
| `songs/{id}.<ext>` | the song files, in the storage format |
| `tmp/` | exports being made for a download |
| `live/{playlist}/hls/` | HLS segments of a live playlist |
| `live/{playlist}/on-air` | the song on air, for a restart |

## 7.3 Config

`maio.toml` (copy `maio.example.toml`):

```toml
listen = "0.0.0.0:8080"
data_dir = "/data"
secure_cookies = false
max_live = 4
crossfade_s = 6.0
moods = ["calm", "focused", "upbeat", "playful", "tense", "dreamy", "melancholy", "triumphant"]
genres = ["lo-fi", "ambient", "synthwave", "jazz", "orchestral", "rock", "electronic", "hip-hop", "funk", "folk", "chillhop", "reggae"]
auto_seconds = [120, 240]

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
# api_key_env = "ACE_STEP_API_KEY"

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

- `[relay] host` and the ports are what the live strip shows listeners; set `host` to the name they reach this machine by.
- `auto_seconds` (default `[120, 240]`) is the shortest and longest length Maio picks from when a request leaves the length on Auto, 15 to 600 seconds (arc42 8.2).
- `log_format = "json"` switches logs to JSON (arc42 8.6); `engine_stall_s` (default 600) fails a song that has not changed for that long; `engine_poll_s` (default 2) is how often a song being made is polled, and `engine_backoff_max_s` (default 30) the longest wait after a busy answer or a failed poll (arc42 8.8).

The environment (`.env`, copy `.env.example`; never printed):

| Variable | What |
|----------|------|
| `MAIO_ADMIN_USER`, `MAIO_ADMIN_PASSWORD` | the one account, made on first start; without them Maio refuses to start |
| `MAIO_API_TOKEN` | optional, at least 32 characters: the API token for scripts |
| `ACE_STEP_API_KEY` | optional: ACE-Step's key, named by `api_key_env` |
| text engine keys | named by each text engine's `api_key_env` |
| `OTEL_EXPORTER_OTLP_ENDPOINT` and the other `OTEL_*` | optional: send traces, metrics and logs over OTLP |
| `RUST_LOG` | log filter, default `info` |
