# Contributing

Thank you for wanting to help. Maio is a personal project with one maintainer, so it moves at that pace, and it stays small on purpose ([Product](docs/product.md) lists what it will not do).

## Before you start

- **Open an issue first** for anything bigger than a typo, so we can agree on the change before you spend time on it.
- **Read the section you are changing.** The [arc42 sections](docs/architecture/01-introduction-and-goals.md) are the authority on what Maio does. A change to behaviour changes its section first, then the code.
- **A surprising decision gets a record.** Start one from the [template](docs/adr/template.md) and add it to [section 9](docs/architecture/09-architecture-decisions.md).

## Setting up

Follow [Development](README.md#development) in the README, and turn on the commit message check once: `git config core.hooksPath .githooks`. `just stack` and `just fake` give you a running Maio without ACE-Step or a GPU.

## Making a change

1. Branch from `main`.
2. Write a test that fails for the right reason, then the code that makes it pass. A fix to the web app gets an end-to-end check.
3. Keep `just test` and `just e2e` green.
4. Write commit messages in [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) form, one logical change per commit: `fix(player): Pause while listening live turns to Play`. Sign each one off with `git commit -s`, which certifies the [Developer Certificate of Origin](https://developercertificate.org).
5. Open a pull request that says what changed for the person using Maio, and why.

The full rules (principles, the layers, scopes for commit messages, secrets and accessibility) are in [Project rules](docs/project-rules.md).

## Conduct

Everyone taking part follows the [Code of conduct](CODE_OF_CONDUCT.md).
