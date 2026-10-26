---
status: accepted
date: 2026-10-21
decision-makers: fulgas
---

# The web app learns of changes from server-sent events, never by polling

## Context and Problem Statement

Some things change with nobody clicking: an engine stops, MediaMTX stops, a listener joins a live playlist, the next song starts, a song is renamed from a script. The web app looked again on timers, every 5 or 30 seconds, so it was late by up to that long, and it asked even when nothing had changed. How should it learn of these changes?

## Considered Options

- Events that carry the new state, sent where the change happens
- Events that say only what changed, and a fetch of it
- Timers in the web app, as before
- One task on the server that compares everything once a second and sends what changed

## Decision Outcome

Chosen option: "Events that carry the new state, sent where the change happens", because every one of these changes has a moment where the server sees it, and the event stream the web app already listens to can carry the change itself. The engine adapter tells when an engine's status changes; a station tells when a song starts, when it picks the next one, and when a listener joins or leaves; the relay adapter tells when a look at MediaMTX finds something new; a request tells what it changed. Each event carries what the API would answer for that thing, built at that moment, and one task builds and tells every event, the adapters' and the requests' alike, one at a time, so the newest state is always told last. A browser that falls too far behind, or connects, loads the page again, since it may have missed an event. The web app keeps the latest state of each thing by id, and only the parts of the page that show it change. Events that only name a change would still send the web app back to ask, once per change and per open page. A task comparing everything once a second would move the polling to the server, not remove it. The one loop left is Maio asking MediaMTX about its readers, since MediaMTX cannot say when a reader leaves.

### Consequences

- Good, because a change shows within a moment, nothing is asked while nothing changes, and nothing is asked when something does.
- Good, because each part of the page changes only for its own events.
- Bad, because a change the server makes must remember to send its event; a forgotten one leaves a screen stale until a reload.
- Bad, because events sent while a browser's stream is down are lost, so a reconnect loads the page once more.

### Confirmation

The server's tests check that a renamed song, a deleted song, a changed or deleted playlist and a playlist going live each arrive as an event carrying the new state, and that a playlist change also tells the songs whose marks changed. The adapters' tests check that a change of engine status, a listener joining or leaving and a new look at MediaMTX are told. The browser tests check that the header and a live playlist's page fetch nothing while nothing changes, and that a song and a playlist renamed elsewhere show at once with nothing fetched.
