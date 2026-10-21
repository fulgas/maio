---
status: accepted
date: 2026-10-21
decision-makers: fulgas
---

# Conventional Commits decide semantic versions

## Context and Problem Statement

Maio has an API that scripts use, a config file and a database, so a release must say whether it can break them. Commit messages already follow Conventional Commits, but nothing checked them, and nothing said how a version number is chosen. How are commits checked, and how are releases numbered?

## Considered Options

- Semantic Versioning, worked out from Conventional Commits, with a commit-msg hook
- Semantic Versioning chosen by hand at each release
- Calendar versions (2026.10)
- A release tool such as semantic-release or cocogitto

## Decision Outcome

Chosen option: "Semantic Versioning, worked out from Conventional Commits, with a commit-msg hook", because the commits already carry what the number needs: a breaking change means a major release (minor while the version starts with 0), a feature a minor one, only fixes or performance work a patch, and anything else no release. A git hook in the repository refuses other messages, with nothing to install. A release sets the crate's version, is committed as a release commit and tagged with the version, signed.

### Consequences

- Good, because a version number tells a script author whether an upgrade can break them, and the notes come from the commit subjects.
- Good, because the check costs one line of setup per clone and no dependency.
- Bad, because the hook is local: a clone that never turns the hook on is not checked.
- Bad, because the number is worked out by a person until a release tool earns its place.

### Confirmation

The documentation tests run the hook against valid and invalid messages and checks that the project rules state the release table.
