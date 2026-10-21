---
status: accepted
date: 2026-10-20
decision-makers: fulgas
---

# Decision records follow MADR 4.0.0

## Context and Problem Statement

The first records used a short format of our own. A known format makes records easier to write, read and check, and keeps the rejected options visible. Which format do decision records use?

## Considered Options

- MADR 4.0.0 with front matter and Confirmation
- Our own short format
- Nygard's original format

## Decision Outcome

Chosen option: "MADR 4.0.0 with front matter and Confirmation", because it is widely used, names the considered options and the chosen one, separates good and bad consequences, and says how compliance is confirmed. Each record starts with its status, date and who decided, then Context and Problem Statement, Considered Options, Decision Outcome with Consequences and Confirmation. New records start from the [template](template.md).

### Consequences

- Good, because every record answers the same questions in the same order.
- Bad, because records are a little longer.

### Confirmation

The documentation tests check every record for its status, date and decision makers, the MADR sections and a matching status in section 9.
