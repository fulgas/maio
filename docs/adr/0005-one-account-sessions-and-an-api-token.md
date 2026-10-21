---
status: accepted
date: 2026-10-19
decision-makers: fulgas
---

# One account, server-side sessions and an API token

## Context and Problem Statement

One person owns Maio; there is no one else to manage. It must never be open, and scripts need access without a browser. How do people and scripts get in?

## Considered Options

- One account from the environment, sessions in a strict cookie, one API token
- Users and roles
- An identity provider (OAuth)

## Decision Outcome

Chosen option: "One account from the environment, sessions in a strict cookie, one API token", because it is safe by default with nothing to manage: the account is made on first start from a user name and password in the environment, or Maio refuses to start; browsers get a server-side session in a cookie that scripts in the page cannot read and other sites cannot send, and every change checks where the request came from; scripts send one token kept with the other secrets.

### Consequences

- Good, because it is never open and needs no user management.
- Bad, because sharing means sharing the account; a second person needs a new decision.

### Confirmation

Tests cover the refusal to start without the account, bad origins, sign-in limits and constant-time token checks; arc42 8.1 lists the rules.
