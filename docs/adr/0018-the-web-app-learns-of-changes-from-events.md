---
status: accepted
date: 2026-10-21
decision-makers: fulgas
---

# The web app learns of changes from server-sent events, never by polling

## Context and Problem Statement

Some things change on their own, with nobody clicking: an engine stops, MediaMTX stops, a listener joins a live playlist, the next song starts. The web app looked again on timers, every 5 or 30 seconds, so it was late by up to that long and asked even when nothing had changed. How should it learn of these changes?

## Considered Options

- Server-sent events for every change, sent where the change happens
- Timers in the web app, as before
- One task on the server that compares everything once a second and sends what changed

## Decision Outcome

Chosen option: "Server-sent events for every change, sent where the change happens", because the server already holds an event stream the web app listens to for jobs, songs and playlists, and every one of these changes has a moment where the server sees it. The engine adapter tells when an engine's health changes; a station tells when a song starts and when a listener joins or leaves; the relay adapter tells when a look at MediaMTX finds something new. The web app reloads only what an event names. The one loop left is Maio asking MediaMTX about its readers, since MediaMTX cannot say when a reader leaves. A task comparing everything once a second would have moved the polling to the server, not removed it.

### Consequences

- Good, because the web app shows a change within a moment, and asks for nothing while nothing changes.
- Good, because a browser that was asleep reloads everything when its event stream reconnects.
- Bad, because a change the server makes must remember to send its event; a forgotten one leaves a screen stale until a reload.

### Confirmation

The browser tests check that the header fetches no status while nothing changes, and that a live playlist's page fetches its live state only when an event says so, while a new listener still shows within seconds. The adapters' tests check that a change of engine health, a listener joining or leaving, and a new look at MediaMTX are told.
