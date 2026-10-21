---
status: accepted
date: 2026-10-19
decision-makers: fulgas
---

# OpenTelemetry across Maio and ACE-Step

## Context and Problem Statement

A slow or failed song crosses Maio's request, the job and ACE-Step. Logs alone do not connect them. How is one song followed end to end?

## Considered Options

- OpenTelemetry over OTLP with traceparent to ACE-Step
- Logs with correlation ids
- A vendor agent

## Decision Outcome

Chosen option: "OpenTelemetry over OTLP with traceparent to ACE-Step", because it gives one trace from the click to the audio with standard tools: with `OTEL_EXPORTER_OTLP_ENDPOINT` set, Maio sends traces, metrics and logs; each request is a span and each job a `job` span under it; calls to ACE-Step carry `traceparent`, and `just engine` runs ACE-Step under zero-code instrumentation.

### Consequences

- Good, because one trace holds the request, the job and ACE-Step's spans.
- Good, because nothing is exported or run when the variable is unset.
- Bad, because a collector is needed to see anything.

### Confirmation

`tests/trace_context.rs` checks that `traceparent` reaches ACE-Step; `tests/logs.rs` keeps secrets out of logs.
