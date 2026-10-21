---
status: accepted
date: 2026-10-21
decision-makers: fulgas
---

# The engine's health is the circuit breaker; a busy engine is retried every 2 s

## Context and Problem Statement

ACE-Step can be busy, slow or gone. Maio must not pile requests on it, and must not accept songs it cannot make. Services usually add a circuit breaker and exponential backoff for this. Does Maio need them, on top of what it has?

## Considered Options

- The engine's health as the breaker, and a fixed 2 s retry when the engine is busy
- A separate circuit breaker, and exponential backoff with jitter

## Decision Outcome

Chosen option: "The engine's health as the breaker, and a fixed 2 s retry when the engine is busy", because Maio already has every part a breaker has. Engine slots cap what reaches ACE-Step, and the limits on waiting songs and on how many you ask for cap what reaches Maio. An engine that is down refuses new songs before a job starts: the breaker is open. A health check every 30 seconds lets one call through and closes it when the engine answers: half-open, then closed. A second breaker would keep a second idea of whether ACE-Step is up. Exponential backoff spreads many clients apart; Maio is one client, for one person, with slots in front of the engine, so a fixed 2-second wait when the engine says it is busy costs nothing. The rules are listed in arc42 3.3.

### Consequences

- Good, because there is one answer to "is ACE-Step up?", shown in the header and used to refuse songs.
- Good, because there is nothing new to configure or test.
- Bad, because an engine that stops answering health checks while it makes a long song would be marked down after 90 seconds and would refuse songs it could have queued. This has not been seen; if it is, a check that times out while the engine is making a song should not count toward down.

### Confirmation

The engine adapter's tests show that an engine nobody answers for is down after three checks, and at once when a call for a song cannot connect. The song-making tests show that a down engine refuses songs, that songs reach a one-slot engine in the order asked, and that a stalled or lost song is tried once more.
