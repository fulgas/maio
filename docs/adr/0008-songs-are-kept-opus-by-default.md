---
status: accepted
date: 2026-10-15
decision-makers: fulgas
---

# Songs are kept; Opus by default, loudness-normalised

## Context and Problem Statement

v0 had a storage budget that evicted songs. Songs made on purpose should not disappear, and songs from one engine run vary in loudness. How are songs stored, and for how long?

## Considered Options

- Kept until deleted, Opus by default, normalised to -16 LUFS
- A storage budget with eviction
- Keep ACE-Step's WAV as it comes

## Decision Outcome

Chosen option: "Kept until deleted, Opus by default, normalised to -16 LUFS", because the library stays predictable and sounds even on air: nothing deletes songs automatically, a song can be deleted only when it is in no playlist, songs are stored in a configured format (Opus at 160 kb/s by default; FLAC, MP3, AAC too) with tags, and downloads convert on request.

### Consequences

- Good, because nothing disappears without being asked.
- Good, because loudness is even across songs and on air.
- Bad, because disk use grows with the library.

### Confirmation

The ffmpeg adapter's tests check normalisation, tags and export; `409 song_in_playlist` guards deletion.
