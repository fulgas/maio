# Maio: the container stack (Maio + MediaMTX) and the ACE-Step engine on this Mac.
# `just` runs both; Ctrl-C stops both.

set dotenv-load := true

ace_root := env("ACE_ROOT", home_directory() / "Projects/fulgas/ACE-Step-1.5")
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

# Run ACE-Step 1.5's own API server in the foreground (models load on first start).
engine:
    #!/usr/bin/env bash
    set -euo pipefail
    # ACE-Step's API key, when .env has one; Maio sends it too when [[engine]] names it in api_key_env.
    key=$(sed -n 's/^ACE_STEP_API_KEY=//p' "{{justfile_directory()}}/.env" 2>/dev/null || true)
    if [ -n "$key" ]; then export ACESTEP_API_KEY="$key"; fi
    cd "{{ace_root}}"
    # Load the models at start: acestep-api otherwise waits for the first song, and Maio waits for the models.
    export ACESTEP_NO_INIT=false ACESTEP_API_HOST=127.0.0.1 ACESTEP_API_PORT={{engine_port}} ACESTEP_DEVICE=mps ACESTEP_LM_MODEL_PATH=acestep-5Hz-lm-1.7B ACESTEP_LM_BACKEND=pt
    if [ -n "${OTEL_EXPORTER_OTLP_ENDPOINT:-}" ]; then
        export OTEL_SERVICE_NAME="${OTEL_SERVICE_NAME_ACE:-ace-step}" OTEL_EXPORTER_OTLP_PROTOCOL=http/protobuf
        exec uv run --with opentelemetry-distro --with opentelemetry-exporter-otlp --with opentelemetry-instrumentation-fastapi opentelemetry-instrument acestep-api
    fi
    exec uv run acestep-api

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

# A real browser through every screen: builds the site, runs the fake engine and a throwaway server, runs e2e/smoke.mjs.
e2e:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo leptos build
    rm -rf target/e2e-data
    # Its own port, so a real ACE-Step on {{engine_port}} never answers the tests.
    node e2e/fake-ace.mjs 7811 & engine=$!
    for _ in $(seq 120); do curl -sf -o /dev/null http://127.0.0.1:7811/health && break; sleep 0.5; done
    LEPTOS_ENV=PROD MAIO_ADMIN_USER=smoke MAIO_ADMIN_PASSWORD=smoke-password-1 target/debug/maio e2e/maio.toml & server=$!
    trap 'kill $engine $server 2>/dev/null || true' EXIT
    for _ in $(seq 60); do curl -sf -o /dev/null http://127.0.0.1:8099/sign-in && break; sleep 0.5; done
    cd e2e && npm install --silent && npx playwright install chromium >/dev/null
    E2E_USER=smoke E2E_PASSWORD=smoke-password-1 node smoke.mjs
