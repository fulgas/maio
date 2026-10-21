---
status: accepted
date: 2026-10-20
decision-makers: fulgas
---

# Product, Design and Tech pages lead into arc42

## Context and Problem Statement

arc42 is the authority ([ADR 0012](0012-arc42-is-the-authority.md)), but twelve sections are a lot to read before a first change, and tools that work on the code look for `product.md`, `DESIGN.md` and `tech.md`. The web app's look also sat in one long bullet of arc42 8.5, mixed with what each screen does. Should Maio have those three pages, and what do they own?

## Considered Options

- Three short pages that summarise and link into arc42; Design owns the look
- Move product, design and tech content out of arc42 into the three pages
- No extra pages

## Decision Outcome

Chosen option: "Three short pages that summarise and link into arc42; Design owns the look", because it gives a one-page way in without a second copy to drift. `docs/product.md` is what Maio is, for whom, its goals and non-goals; `DESIGN.md`, at the repository root in the [DESIGN.md format](https://github.com/google-labs-code/design.md/blob/main/docs/spec.md), is the look: tokens for colour in both themes, type, spacing, shapes and components, with the reasons in prose; arc42 8.5 points to it; `docs/tech.md` is the stack, the code layout and the `just` commands. Where a page disagrees with a section, the section wins, except for the look.

### Consequences

- Good, because a newcomer, or a tool, gets the whole picture in three pages.
- Good, because the look has one home, beside the screens it styles.
- Bad, because the summaries must follow when a section changes.

### Confirmation

`docs-site/tests` checks that each page links into arc42 and arc42 1.5 links back, that `DESIGN.md` has the format's sections in order and a token for every colour in `style/main.css`, and that every `just` recipe appears in `docs/tech.md`.
