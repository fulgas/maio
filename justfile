# Maio: the container stack (Maio + MediaMTX) and the ACE-Step engine on this Mac.
# `just` runs both; Ctrl-C stops both.

set dotenv-load := true

ace_root := env("ACE_ROOT", home_directory() / "Projects/fulgas/ACE-Step-1.5")
engine_root := env("ENGINE_ROOT", home_directory() / "Projects/fulgas/maio-engine")
engine_port := "7801"

# Start the stack, then run ACE-Step in this terminal; Ctrl-C stops the engine and the stack.
run:
    #!/usr/bin/env bash
    set -euo pipefail
    just stack
    trap 'echo; echo "stopping the stack"; docker compose stop' EXIT
    just engine

# Start Maio and MediaMTX in the background (rebuilds when the code changed).
stack:
    docker compose up -d --build
    @echo "Maio on http://localhost:8080"

# Run the ACE-Step engine in the foreground.
engine:
    cd "{{engine_root}}" && uv run --extra ace-step maio-engine --backend ace-step --ace-root "{{ace_root}}" --port {{engine_port}}

# Run the fake engine instead (sine tones, no GPU), for trying things quickly.
fake:
    cd "{{engine_root}}" && uv run maio-engine --backend fake --port {{engine_port}}

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

# A real browser through every screen: builds the site, runs the fake engine and a throwaway server, runs e2e/smoke.mjs.
e2e:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo leptos build
    rm -rf target/e2e-data
    (cd "{{engine_root}}" && exec uv run maio-engine --backend fake --port {{engine_port}}) & engine=$!
    for _ in $(seq 120); do curl -sf -o /dev/null http://127.0.0.1:{{engine_port}}/health && break; sleep 0.5; done
    LEPTOS_ENV=PROD MAIO_ADMIN_USER=smoke MAIO_ADMIN_PASSWORD=smoke-password-1 target/debug/maio e2e/maio.toml & server=$!
    trap 'kill $engine $server 2>/dev/null || true' EXIT
    for _ in $(seq 60); do curl -sf -o /dev/null http://127.0.0.1:8099/sign-in && break; sleep 0.5; done
    cd e2e && npm install --silent && npx playwright install chromium >/dev/null
    E2E_USER=smoke E2E_PASSWORD=smoke-password-1 node smoke.mjs
