# Product

**M**usic + **AI** + **O**utput: that is what the name Maio says.

Maio makes songs with music generation models, keeps them in your own library, lets you build playlists from them, and plays them: on demand in the browser, or as live streams anyone with the link can tune in to. The [architecture](architecture/01-introduction-and-goals.md) is the authority; this page is the short version.

## Who it is for

- **You, the owner**: one person runs it and owns everything in it. Make songs, keep them, build playlists, play them, put them on air.
- **Listeners**: tune in to a live playlist from any player, with no account.
- **Scripts**: do anything the web app does through the JSON API with the API token ([arc42 3.5](architecture/03-system-scope-and-context.md#35-the-http-api)).

## What it does

| Thing | What it is |
|-------|------------|
| **Song** | Made from one prompt by a music engine, then kept until you delete it. |
| **Playlist** | A named, ordered collection of songs you choose. |
| **Player** | Plays songs and playlists in your browser; a live playlist plays its stream. |
| **Live** | A switch on a playlist: it streams continuously, looping, to anyone with its link. |

The rules (what can be deleted, how many playlists can be live) are in [arc42 1.2](architecture/01-introduction-and-goals.md#12-the-model-and-its-rules); who and what Maio talks to is in [arc42 3](architecture/03-system-scope-and-context.md).

## Goals, in order

1. **Simple to run**: one container plus MediaMTX, one config file, one SQLite file.
2. **Changeable**: the rules know nothing about HTTP, SQLite, ffmpeg or Leptos.
3. **Never open**: no start without an account; every call needs a session or the API token.
4. **Usable by everyone**: WCAG 2.1 AA, from 360 px wide.
5. **A stream that keeps going**: live playlists play on the server and survive restarts.

How each goal is checked: [arc42 10](architecture/10-quality-requirements.md).

## Not goals

- A second user, roles or sharing an account ([ADR 0005](adr/0005-one-account-sessions-and-an-api-token.md)).
- Songs made in the background, or deleted to save space ([ADR 0001](adr/0001-songs-and-playlists-replace-the-radio.md), [ADR 0008](adr/0008-songs-are-kept-opus-by-default.md)).
- Scaling out to many machines ([ADR 0003](adr/0003-one-binary-one-sqlite-file.md)).
