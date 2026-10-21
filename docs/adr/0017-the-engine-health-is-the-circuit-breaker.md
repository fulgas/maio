---
status: accepted
date: 2026-10-21
decision-makers: fulgas
---

# The engine's health is the circuit breaker; trouble is retried with exponential backoff

## Context and Problem Statement

ACE-Step can be busy, slow or gone. Maio must not pile requests on it, and must not accept songs it cannot make. Services usually add a circuit breaker and exponential backoff for this. What should Maio use, on top of what it has?

## Considered Options

- The engine's health as the breaker, and exponential backoff from a proven crate when the engine is busy or a poll fails
- The engine's health as the breaker, and exponential backoff written in Maio
- The engine's health as the breaker, and a fixed 2-second retry
- A separate circuit breaker library as well

## Decision Outcome

Chosen option: "The engine's health as the breaker, and exponential backoff from a proven crate when the engine is busy or a poll fails", because Maio already has every part a breaker has, and backoff stops it asking a struggling engine every 2 seconds. Engine slots cap what reaches ACE-Step, and the limits on waiting songs and on how many you ask for cap what reaches Maio. An engine that is down refuses new songs before a job starts: the breaker is open. A health check every 30 seconds lets one call through and closes it when the engine answers. A second breaker would keep a second idea of whether ACE-Step is up. When ACE-Step says it is busy, or a poll gets no answer, each wait doubles from the poll interval up to a most, both set in the config. The backon crate runs these retries, the waits and the limit on tries: retrying is a solved problem, and a well-used crate that does that job beats a loop of our own that would need its own tests and fixes. It runs without jitter, since Maio is one client and there is nothing to spread apart. While ACE-Step answers, a song is polled at a steady pace, so its progress moves smoothly and a finished song is seen at once. The rules, and how they behave, are in [arc42 8.8](../architecture/08-crosscutting-concepts.md#88-when-the-engine-is-busy-or-down).

### Consequences

- Good, because there is one answer to "is ACE-Step up?", shown in the header and used to refuse songs.
- Good, because a struggling engine is asked less and less often, and the backoff is a well-used crate's, not ours.
- Bad, because one more dependency.
- Bad, because an engine that stops answering health checks while it makes a long song would be marked down after 90 seconds and would refuse songs it could have queued. This has not been seen; if it is, a check that times out while the engine is making a song should not count toward down.

### Confirmation

The engine adapter's tests show that an engine nobody answers for is down after three checks, and at once when a call for a song cannot connect. The song-making tests show that a down engine refuses songs, that a busy engine is asked again after waits that double up to the most, that failed polls wait longer each time and a good answer brings the steady pace back, and that a stalled or lost song is tried once more.
