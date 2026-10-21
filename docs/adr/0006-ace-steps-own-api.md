---
status: accepted
date: 2026-10-19
decision-makers: fulgas
---

# ACE-Step's own API, no wrapper

## Context and Problem Statement

Maio first spoke its own contract to `maio-engine`, a Python server wrapping ACE-Step. ACE-Step 1.5 ships `acestep-api`, which does what the wrapper did (health, queue, generation, results, audio, lyrics, model switching) and more. Keep the wrapper?

## Considered Options

- Talk to acestep-api directly
- Keep the wrapper and its contract

## Decision Outcome

Chosen option: "Talk to acestep-api directly", because it removes a program to maintain and shows ACE-Step's progress, stages and models as they are. `just engine` starts ACE-Step from its checkout with the models loaded, an optional API key and OpenTelemetry when configured.

### Consequences

- Good, because there is one less program to keep in step with ACE-Step.
- Bad, because Maio depends on ACE-Step's response shapes (lost tasks answer `"[]"`, results are JSON strings).

### Confirmation

The engine adapter's tests use ACE-Step's real response shapes, and so does the e2e fake (`e2e/fake-ace.mjs`).
