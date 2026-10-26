## What changes, for the person using Maio

<!-- One logical change. Link issues with "Fixes #123". -->

## Checklist

- [ ] The arc42 section this touches says what the change does, in this pull request or before it
- [ ] A failing test came first; a fix to the web app has a check in `e2e/smoke.mjs`
- [ ] `just test` passes
- [ ] `just e2e` passes, for any change to the web app, the API or the streams
- [ ] A decision that would surprise the next reader has a record in `docs/adr/`
- [ ] Every commit is a signed [Conventional Commit](https://www.conventionalcommits.org/en/v1.0.0/), with a `BREAKING CHANGE:` footer when it breaks the API, the config or stored data
