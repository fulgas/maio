---
status: accepted
date: 2026-10-20
decision-makers: fulgas
---

# arc42 is the authority

## Context and Problem Statement

A single specification file said what Maio must do in one long file, beside implementation plans. It mixed requirements, design and reference, and had no place for decisions and their reasons. Where does the architecture live?

## Considered Options

- arc42 sections with ADRs and a C4 model
- Keep SPEC.md and add arc42 beside it
- MkDocs Material for the site
- A static site generator of our own

## Decision Outcome

Chosen option: "arc42 sections with ADRs and a C4 model", because it is a structure readers know and keeps decisions with their reasons: the twelve arc42 sections replace the specification, section 9 indexes the ADRs, and the documentation is published as a standalone website with an interactive C4 explorer drawn from one model and a reader rendered at build time (built with Zola since [ADR 0016](0016-zola-builds-the-documentation-site.md)). A section embeds a C4 view by naming it; Mermaid draws sequence and state diagrams. Implementation plans are not kept in the repository.

### Consequences

- Good, because changing behaviour means changing the right section first, and code points to the section it follows.
- Bad, because the C4 model is written by hand and must change with the code.

### Confirmation

The tests build the site and check every link, every model reference into the documentation and every view.
