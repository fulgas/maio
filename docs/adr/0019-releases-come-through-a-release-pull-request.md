---
status: accepted
date: 2026-10-26
decision-makers: fulgas
---

# Releases come through a release pull request

## Context and Problem Statement

Maio is now published on GitHub, and people who run it need versions they can download and a changelog that says what changed. The commits already decide the version number (ADR 0015), but a person worked it out, wrote the notes and made the tag by hand, and nothing was built for anyone to download. How is a release prepared, made and shipped?

## Considered Options

- A release pull request kept up to date by a Rust release tool, merged by the owner, then tagged and released by the project's release app
- A release made straight from every qualifying push to the main branch, with no pull request
- A release command run by hand on the owner's machine, which signs the tag with the owner's key

## Decision Outcome

Chosen option: "A release pull request kept up to date by a Rust release tool, merged by the owner, then tagged and released by the project's release app", because the next version and its changelog can be read and checked before anything is released, the tests run on the exact commit that becomes the release, and nobody works out a number or writes notes by hand. Only changes the person using Maio would notice open a release: features, fixes, performance work and breaking changes. Merging the pull request is the decision to release; the release app then makes the tag and the GitHub Release, the server with its web app is built for Linux on both common processors and for Macs with Apple silicon and attached to it, and the container image is published for both processors; the release is published only once all of it is attached, since a published release can no longer change. Releasing straight from the main branch would leave no moment to read the changelog before it ships. Signing by hand keeps the owner's key the only signer, but turns every release back into manual steps.

### Consequences

- Good, because a release is one reviewed merge, and its notes and number follow from the commits with nothing to type.
- Good, because every release comes as a download and as a container image, so nobody has to build Maio to run it.
- Bad, because release tags are made by the release app through GitHub, not signed with the owner's key; the app is trusted to bypass the tag rules.
- Bad, because the release app's credentials live in the repository's secrets, so whoever can change the workflows can use them.

### Confirmation

The release workflow runs on every push to the main branch and keeps one release pull request open; the project rules describe the steps, and the tag rules let only the owner and the release app create tags.
