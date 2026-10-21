---
status: accepted
date: 2026-10-21
decision-makers: fulgas
---

# Zola builds the documentation site

## Context and Problem Statement

The documentation site was built by our own Node script: it rendered the Markdown, drew the C4 previews, folder trees and DESIGN.md's tokens, and wrote a one-page app. It worked, but it was a renderer of our own to maintain. Should a standard static site generator build the site, and what has to stay ours?

## Considered Options

- Zola, fed by a sync step that keeps the docs unchanged
- Keep our own build script
- Zola with the docs rewritten for it (front matter in every file, `@/` links)

## Decision Outcome

Chosen option: "Zola, fed by a sync step that keeps the docs unchanged", because Zola renders the pages, checks every link between documents and its anchor at build time, and serves with live reload, while the Markdown stays readable as it is on GitHub. `docs-site/sync.mjs` copies `docs/` into Zola's `content/`: it adds the front matter Zola needs, moves the MADR keys under `extra`, gives headings our anchors, turns our relative links into Zola's checked `@/` links, and draws the C4 previews, folder trees, DESIGN.md's tokens, the decision-record status line and inlined SVG pictures, as before. The templates reproduce the reader, and the C4 explorer stays our app on the home page. Zola is pinned at 0.23.6; `just zola` downloads it once into `docs-site/.zola`.

### Consequences

- Good, because a standard tool builds the site and checks its links, and `just docs-serve` reloads as you edit.
- Good, because the site matches the previous one page for page; a comparison of all 36 pages in dark, light and phone width found no pixel different.
- Bad, because the whole site in one file (`architecture.html`) is gone: Zola writes a folder of pages.
- Bad, because Zola's template language changed in 0.23 (Tera 2), so the pin matters, and the sync step is still code of ours.

### Confirmation

`docs-site/tests` builds the site with the pinned Zola and checks the built pages and the Markdown: links, C4 previews, folder trees, DESIGN.md's tokens, the decision records, the header's version.
