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

Chosen option: "OpenTelemetry over OTLP with traceparent to ACE-Step", because it gives one trace from the click to the audio with standard tools: with an OTLP endpoint set, Maio sends traces, metrics and logs; each request is a span and each song being made a span under it; calls to ACE-Step carry the trace context, and ACE-Step runs under zero-code instrumentation.

### Consequences

- Good, because one trace holds the request, the job and ACE-Step's spans.
- Good, because nothing is exported or run when the variable is unset.
- Bad, because a collector is needed to see anything.

### Confirmation

Tests check that the trace context reaches ACE-Step and that no secret reaches the logs.
