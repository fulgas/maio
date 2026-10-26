# 1. Introduction and goals

## 1.1 What Maio is

Maio makes songs with music generation models, keeps them in your own library, lets you build playlists from them, and plays them, on demand in the browser or as live streams anyone with the link can tune in to.

It is a personal tool: one person runs it and owns everything in it. This version (v1) replaced v0, a radio with stations, queues and roles ([ADR 0001](../adr/0001-songs-and-playlists-replace-the-radio.md)); v1 started on a fresh database.

## 1.2 The model and its rules

| Thing | What it is |
|-------|------------|
| **Song** | Made from one prompt by a music engine, then kept in the library until you delete it. |
| **Playlist** | A named, ordered collection of songs you choose. A song can be in many playlists, or in none. Playlists have no prompts and make no songs themselves. |
| **Player** | Plays songs and playlists on demand in your browser, with shuffle, repeat one and repeat all; a live playlist plays its stream. |
| **Live** | A switch on a playlist. While it is on, the playlist streams continuously, looping, to anyone with its link. |

Rules:

- One prompt makes one song. Making many songs from one prompt is a client convenience, not part of the server.
- A new song belongs to no playlist. You add it to playlists later.
- A song is kept until you delete it, and you can only delete a song that is in no playlist (`409 song_in_playlist`, naming the playlists). Nothing deletes songs automatically: there is no storage budget and no retention sweep.
- A playlist that is live cannot be deleted (`409 playlist_live`); take it off air first.
- At most `max_live` playlists are live at once (default 4): every live stream runs its own encoders. Going live beyond that is refused with `409 live_limit`, naming the live playlists.

## 1.3 Quality goals

| Priority | Goal | What it means here |
|----------|------|--------------------|
| 1 | Simple to run | One container plus MediaMTX, one config file, one SQLite file. ACE-Step runs as its own server, beside it. |
| 2 | Changeable | A hexagon in one crate: the rules know nothing about HTTP, SQLite, ffmpeg or Leptos, and `tests/layers.rs` keeps it so. |
| 3 | Never open | No start without an account; every API call needs the session or the API token; secrets never reach a log. |
| 4 | Usable by everyone | WCAG 2.1 AA: keyboard throughout, visible focus, 4.5:1 text contrast, from 360 px wide. |
| 5 | A stream that keeps going | Live playlists play on the server, survive restarts with the song on air, and say when something is wrong. |
| 6 | Up to date and resilient | Every open page shows a change as it happens, with nothing polled; a busy or stopped engine is waited for or refused, never flooded, and a song is never lost without saying so. |

## 1.4 Stakeholders

| Who | Wants |
|-----|-------|
| You, the owner | Make songs, keep them, build playlists, play them, put them on air. |
| Listeners | Tune in to a live playlist from any player, with no account. |
| Scripts and other programs | Do anything the web app does through the JSON API with the API token (arc42 3.5). |
| Whoever changes the code | One place that says what Maio must do (this documentation) and code that matches it. |

## 1.5 This documentation

These twelve arc42 sections are the authority: what Maio must do and how it is built. Code points to them as `arc42 x.y` (section and subsection). The [decisions](09-architecture-decisions.md) say why, the [C4 model](#view/context) draws the parts, and the [project rules](../project-rules.md) say how changes are made: a change to what Maio does starts in its section here.

Three short pages lead into these sections: [Product](../product.md) (what Maio is, for whom, its goals and non-goals), [Design](../../DESIGN.md) (the web app's look: colours, type, shape, components) and [Tech](../tech.md) (the stack, the code layout and the commands). They summarise and link; where one disagrees with a section here, the section wins, except for the look, which Design owns ([ADR 0014](../adr/0014-product-design-and-tech-pages.md)).

The documentation follows [Diátaxis](https://diataxis.fr): these sections and the decision records are the explanation; [a tutorial](../tutorials/01-your-first-song-on-air.md) and [how-to guides](../how-to/01-run-ace-step-for-maio.md) are for doing things; [Reference](../reference.md) says where to look things up.

