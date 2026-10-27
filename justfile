# Maio: the container stack (Maio + MediaMTX). ACE-Step runs as its own server (docs/how-to/01-run-ace-step-for-maio.md).
# `just` starts the stack.

set dotenv-load := true

engine_port := "7801"
zola_version := "0.23.6"
zola := justfile_directory() / "docs-site/.zola/zola"

# Start Maio, built from this checkout, and MediaMTX in the background (rebuilds when the code changed).
stack:
    docker compose -f compose.yaml -f compose.build.yaml up -d --build
    @echo "Maio on http://localhost:8080"

# Run the fake ACE-Step instead (sine tones, no GPU), for trying things quickly.
fake:
    node e2e/fake-ace.mjs {{engine_port}}

# Stop the stack.
stop:
    docker compose stop

# Follow Maio's log.
logs:
    docker compose logs -f maio

# The web app with live reload: rebuilds server and browser on save (needs a local maio.toml).
dev:
    cargo leptos watch

# Every test (needs ffmpeg and ffprobe on PATH; the engine's tests live in its own repo), plus the browser build.
test:
    cargo fmt --check
    cargo test
    cargo build --lib --target wasm32-unknown-unknown --no-default-features --features hydrate
    just zola
    node --test docs-site/tests/*.test.mjs

# A real browser through every screen: builds the site, runs the fake engine and a throwaway server, runs e2e/smoke.mjs.
# With SCREENS=1 it also takes the how-to guides' pictures (e2e/manual.mjs).
e2e:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo leptos build
    rm -rf target/e2e-data target/e2e-relay-data
    # Its own port, so a real ACE-Step on {{engine_port}} never answers the tests.
    node e2e/fake-ace.mjs 7811 & engine=$!
    node e2e/fake-mediamtx.mjs 7812 & mediamtx=$!
    for _ in $(seq 120); do curl -sf -o /dev/null http://127.0.0.1:7811/health && break; sleep 0.5; done
    for _ in $(seq 60); do curl -sf -o /dev/null http://127.0.0.1:7812/v3/info && break; sleep 0.5; done
    LEPTOS_ENV=PROD MAIO_ADMIN_USER=smoke MAIO_ADMIN_PASSWORD=smoke-password-1 target/debug/maio e2e/maio.toml & server=$!
    # A second server, with MediaMTX set up, for the checks that need it.
    LEPTOS_ENV=PROD MAIO_ADMIN_USER=smoke MAIO_ADMIN_PASSWORD=smoke-password-1 target/debug/maio e2e/maio-relay.toml & relayed=$!
    trap 'kill $engine $mediamtx $server $relayed 2>/dev/null || true' EXIT
    for _ in $(seq 60); do curl -sf -o /dev/null http://127.0.0.1:8099/sign-in && break; sleep 0.5; done
    for _ in $(seq 60); do curl -sf -o /dev/null http://127.0.0.1:8098/sign-in && break; sleep 0.5; done
    cd e2e && npm install --silent && npx playwright install chromium >/dev/null
    E2E_USER=smoke E2E_PASSWORD=smoke-password-1 node smoke.mjs
    # SCREENS=1: then the pictures for the how-to guides, into docs/images/manual.
    if [ "${SCREENS:-}" = 1 ]; then E2E_USER=smoke E2E_PASSWORD=smoke-password-1 node manual.mjs; fi

# Zola, pinned, downloaded once into docs-site/.zola: nothing to install by hand.
zola:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -x "{{zola}}" ] && "{{zola}}" --version | grep -q "{{zola_version}}"; then exit 0; fi
    case "$(uname -s)-$(uname -m)" in
        Darwin-arm64) target=aarch64-apple-darwin ;;
        Darwin-x86_64) target=x86_64-apple-darwin ;;
        Linux-x86_64) target=x86_64-unknown-linux-gnu ;;
        Linux-aarch64) target=aarch64-unknown-linux-gnu ;;
        *) echo "no Zola {{zola_version}} download for $(uname -s)-$(uname -m)" >&2; exit 1 ;;
    esac
    mkdir -p "$(dirname "{{zola}}")"
    curl -fsSL "https://github.com/getzola/zola/releases/download/v{{zola_version}}/zola-v{{zola_version}}-$target.tar.gz" | tar xz -C "$(dirname "{{zola}}")"

# The architecture website (C4 explorer, arc42, ADRs) into docs-site/dist: sync.mjs writes Zola's content from docs/, then Zola builds it.
docs: zola
    node docs-site/sync.mjs
    "{{zola}}" --root docs-site build

# Builds the website and serves it on http://localhost:8000 with live reload (DOCS_PORT to change it); re-run after editing docs/.
docs-serve: docs
    "{{zola}}" --root docs-site serve --interface 0.0.0.0 --port "${DOCS_PORT:-8000}" --base-url localhost
