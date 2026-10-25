# 6. Runtime view

## 6.1 Making a song

```mermaid
sequenceDiagram
    autonumber
    participant B as Browser or script
    participant E as maio
    participant T as Text engine
    participant A as ACE-Step (acestep-api)
    B->>E: POST /api/v1/songs (prompt, settings)
    E->>E: check the request (arc42 8.2) and the limits, pick the first healthy engine
    E-->>B: 202 job_id, eta_s
    Note over E: job: waiting, then making
    opt vocals and no lyrics
        E->>T: write lyrics (30 s)
        alt fails, empty or too slow
            E->>A: POST /v1/create_sample
            A-->>E: lyrics
        else
            T-->>E: lyrics
        end
    end
    E->>A: POST /release_task (prompt, lyrics, length, bpm, seed)
    A-->>E: task id
    loop every engine_poll_s, 2 s by default, until done
        E->>A: POST /query_result
        A-->>E: progress, stage
        E-->>B: SSE job: making, progress
    end
    E->>A: GET /v1/audio?path=…
    A-->>E: WAV
    E->>E: ffmpeg: -16 LUFS, Opus, tags
    E->>E: title in layers: person, engine, lyrics, words
    E-->>B: SSE song, job: done
```

- Jobs run one at a time per engine slot, in the order asked; a waiting or making job can be cancelled (ACE-Step finishes it; nobody fetches it).
- A lost task (ACE-Step answers an empty result) or a stall (`engine_stall_s`) fails the job, which is tried once more on another engine when there is one.
- The whole job is one `job` span under the request's span; ACE-Step's spans join through `traceparent` (arc42 8.6).

## 6.2 Signing in

```mermaid
sequenceDiagram
    participant B as Browser
    participant E as maio
    participant D as Library (SQLite)
    B->>E: POST /api/v1/session (username, password, remember), Origin
    E->>E: Origin matches the host?
    alt 5 failures in the last 15 minutes
        E-->>B: 429 rate_limited, Retry-After
    else
        E->>D: the account's Argon2id hash
        E->>E: check the password (same time for a wrong name)
        alt wrong username or password
            E-->>B: 401 bad_credentials
        else
            E->>D: new session (32 random bytes, hashed)
            E-->>B: 204, maio_session cookie (12 h, or 30 days)
        end
    end
```

A script sends `Authorization: Bearer <MAIO_API_TOKEN>` with each call instead: no session and no `Origin` check (arc42 8.1).

## 6.3 Going live and playing out

```mermaid
sequenceDiagram
    participant U as You
    participant E as maio (use cases)
    participant S as Station (playout, mixer, encoders)
    participant M as MediaMTX
    participant L as Listeners
    U->>E: PUT /playlists/{id}/live {"live": true}
    E->>E: under max_live? save the playlist
    E->>S: play it (songs in order, shuffle, address)
    S->>S: pick the next song, decode it while it plays, crossfade, remember it on air
    S->>M: publish AAC to live/{playlist} or listen/{key}
    L->>E: GET /live/{playlist}/stream.mp3 (or AAC, HLS)
    E-->>L: the stream, ICY titles if asked
    L->>M: read over RTSP, SRT or RTMP
    M->>E: POST /relay/auth (may this reader read?)
    E-->>M: yes, a live path
    M-->>L: the stream
    U->>E: PUT /playlists/{id}/live {"live": false}
    E->>S: stop: encoders end, listeners disconnected, the remembered song forgotten
```

- An edit to the playlist reaches the station at once; a new key or a visibility change ends open streams and kicks MediaMTX's readers of the old path.
- After a restart, every playlist that was live comes back on air, starting with the song that was on air.

## 6.4 Listening live in the player

```mermaid
sequenceDiagram
    participant P as Player (browser)
    participant E as maio
    P->>E: GET /api/v1/playlists/{id}/live
    E-->>P: live, MP3 address, on air
    P->>E: GET /listen/{key}/stream.mp3 (the audio element)
    E-->>P: the stream, at the live edge
    loop each time the song on air changes
        E-->>P: event live, over /api/v1/events, with the song on air
    end
    Note over P: Pause hangs up: the connection closes, so a paused listener is not counted
    alt the stream ends
        P->>E: GET /api/v1/playlists/{id}/live
        alt still live (a new address after a visibility change)
            E-->>P: the new address, and the player opens it
        else off air
            E-->>P: not live, and the player stops and says so
        end
    end
```

## 6.5 Watching MediaMTX

```mermaid
sequenceDiagram
    participant R as Relay (maio)
    participant M as MediaMTX control API
    participant B as Browser (header)
    loop every 5 s
        R->>M: GET /v3/info, GET /v3/paths/list (as maio)
        alt answers
            M-->>R: version, started, paths with readers, bytes, broken frames
        else no answer
            R->>R: not answering
        end
    end
    R-->>B: event relay, over /api/v1/events, when a check finds something new: state, version, time up, each live playlist on MediaMTX
    Note over B: the MediaMTX line turns grey and a toast says so, another toast when it is back
```

The password Maio uses with MediaMTX is made up at each start; MediaMTX checks it through the same auth hook (arc42 8.1).
