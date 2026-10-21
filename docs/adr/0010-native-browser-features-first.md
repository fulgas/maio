---
status: accepted
date: 2026-10-20
decision-makers: fulgas
---

# Native browser features first

## Context and Problem Statement

thaw's dialogs did not move focus in, let Tab walk behind the modal and had no accessible name; custom menus and panels repeat what browsers now do well. Accessibility is a quality goal (arc42 1.3). How are dialogs and panels built?

## Considered Options

- The browser's own <dialog> and popover
- Fix focus handling in thaw's dialogs
- A focus-trap library

## Decision Outcome

Chosen option: "The browser's own <dialog> and popover", because the platform already does it right: every dialog is the browser's `<dialog>` opened with `showModal()` (a small `Modal` component named by its title), the engine and MediaMTX panels use the popover API, and announcements go to a `role="status"` region. thaw stays for looks and toasts.

### Consequences

- Good, because focus, Escape, the inert page behind and focus restoration come from the browser.
- Bad, because very old browsers are not supported.

### Confirmation

The e2e suite checks that dialogs take focus, keep the page behind out of reach and give focus back, and that toasts are announced.
