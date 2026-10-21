---
status: accepted
date: 2026-10-16
decision-makers: fulgas
---

# Songs and playlists replace the radio

## Context and Problem Statement

v0 was a radio: radios with a dial, stations that made songs on demand, a queue with two sides, requests to play next, history and favourites, built-in and smart playlists, users and roles, and a storage budget with a retention sweep. It was more machinery than one person needed, and its model (a radio always generating) did not match how songs were wanted: made on purpose, kept, arranged. What should Maio be built around?

## Considered Options

- Songs and playlists, with any playlist able to go live
- Keep the radio and add playlists
- Keep the radio and migrate its data to a new model

## Decision Outcome

Chosen option: "Songs and playlists, with any playlist able to go live", because it is the smallest model that covers what is wanted: a song made from one prompt and kept, a playlist you arrange, and a live switch that does what a station did. Removed from v0: radios and the radio dial, stations, the queue and its sides, asking for a song to play next, history and "Just played", favourites and Kept, the built-in Crate, Kept and Requests playlists, smart and generative playlists, users, roles and the admin flag, the storage budget and the retention sweep. v0's database is not migrated; v1 starts fresh.

### Consequences

- Good, because there is far less code and fewer rules.
- Good, because nothing makes songs in the background and nothing deletes them.
- Bad, because v0's library is not carried over.

### Confirmation

The domain has `Song`, `Playlist` and `Live` and nothing for radios, stations or queues; arc42 1.2 lists the rules.
