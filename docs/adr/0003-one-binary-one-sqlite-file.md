---
status: accepted
date: 2026-10-16
decision-makers: fulgas
---

# One binary, one SQLite file, no broker

## Context and Problem Statement

Maio is personal: one person, one machine. Every extra process is something to start, watch and upgrade. What does it run on?

## Considered Options

- One Rust binary and one SQLite file
- Postgres and a job queue

## Decision Outcome

Chosen option: "One Rust binary and one SQLite file", because it is all one person needs: one binary serves the web app, the JSON API, events and the live streams, and runs the jobs and the playout in-process; the library is one SQLite file in the data volume; background work is tokio tasks. The only companion is MediaMTX, for protocols Maio does not speak (ADR 0007).

### Consequences

- Good, because there is nothing to operate beyond a compose file.
- Bad, because jobs in flight are lost on a restart (ACE-Step's tasks are then reported lost).
- Bad, because it does not scale out, which is not a goal.

### Confirmation

`compose.yaml` has two services, maio and mediamtx; `Cargo.toml` has no database server or broker client.
