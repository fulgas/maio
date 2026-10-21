---
status: accepted
date: 2026-10-19
decision-makers: fulgas
---

# Live playout on the server; MediaMTX as the relay

## Context and Problem Statement

A live playlist must sound the same to every listener and keep playing with nobody watching. Listeners use browsers (MP3, AAC, HLS) and media tools that want RTSP, SRT or RTMP. Where does the playout run, and who speaks those protocols?

## Considered Options

- Playout in Maio, MediaMTX as the relay
- Icecast or Liquidsoap
- RTSP, SRT and RTMP written in Maio

## Decision Outcome

Chosen option: "Playout in Maio, MediaMTX as the relay", because it gives every protocol people ask for with one encode per format: songs decode while they play, an own two-deck mixer crossfades them onto a PCM bus, one ffmpeg per format encodes MP3, AAC and HLS, and Maio publishes AAC to MediaMTX, answers its auth hook, counts and kicks its readers, and checks its control API every 5 s (arc42 8.4, 6.5).

### Consequences

- Good, because Maio owns the crossfade and the order, and MediaMTX does not transcode.
- Bad, because MediaMTX is a second container with its own ports.

### Confirmation

Tests cover the play order, the mixer, the stations, the relay's auth, status and kicks; the e2e suite goes live, listens and goes off air.
