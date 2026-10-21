# Security

## Supported versions

Only the latest commit on `main` gets security fixes.

## Reporting a vulnerability

Please do not open a public issue. Report it privately with GitHub's [private vulnerability reporting](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability): **Report a vulnerability** on the repository's Security tab.

Include what an attacker can do, the steps to reproduce it, and the version or commit. This is a one-person project: reports are read as soon as possible, a fix is agreed with you before anything is published, and you are credited unless you prefer not to be.

## What is in scope

- Signing in, sessions and the API token.
- Reaching anything without signing in, apart from the live streams of public playlists and private ones by their link.
- MediaMTX's auth hook: publishing, or reading a path that is not live.
- Reading or writing files outside the data folder through the API or the stream routes.
- Secrets reaching logs, traces or responses.

How Maio protects these is in [arc42 8.1](docs/architecture/08-crosscutting-concepts.md#81-signing-in-and-security).

## Running it safely

- Use a strong password (10 to 128 characters) and, for scripts, an API token of at least 32 characters. Keep both in `.env`, which is never committed.
- Serve it over HTTPS when it leaves your machine, and set `secure_cookies = true`; behind a reverse proxy that changes the host, set `public_origin`.
- Leave MediaMTX's control API (port 9997) unpublished, as `compose.yaml` does.
