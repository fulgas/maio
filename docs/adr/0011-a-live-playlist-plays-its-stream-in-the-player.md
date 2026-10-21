---
status: accepted
date: 2026-10-20
decision-makers: fulgas
---

# A live playlist plays its stream in the player

## Context and Problem Statement

Pressing Play on a live playlist started song 1 in the browser, not what listeners heard, and the player's shuffle and skip suggested control over a broadcast. What should Play do on a live playlist?

## Considered Options

- Play tunes into the stream
- Keep them separate: Play starts song 1 locally
- Ask on each press
- Let the player steer the stream for everyone

## Decision Outcome

Chosen option: "Play tunes into the stream", because "live" then means listening in, everywhere: the bar shows Live and the song on air, seek, skip, shuffle and repeat are off, Pause hangs up and Play joins at the live edge. A song's own Play and playlists that are not live stay on demand; the player bar shows only while it holds something, and a live playlist's band has no player shuffle.

### Consequences

- Good, because you hear what listeners hear.
- Good, because nobody skips for every listener by accident; the stream's order changes only in the live strip.
- Bad, because a live playlist cannot be played from the start in one press (a song's own Play still can).

### Confirmation

The e2e suite plays a live playlist's stream, pauses and resumes it, plays a song on demand from it, and sees the player stop when it goes off air.
