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

# Run the ACE-Step engine in the foreground; it reads its token from .env.
engine:
    cd engine && MAIO_ENGINE_TOKEN="$MAIO_ACE_STEP_TOKEN" uv run --extra ace-step maio-engine --backend ace-step --ace-root "{{ace_root}}" --port {{engine_port}}

# Run the fake engine instead (sine tones, no GPU), for trying things quickly.
fake:
    cd engine && MAIO_ENGINE_TOKEN="$MAIO_ACE_STEP_TOKEN" uv run maio-engine --backend fake --port {{engine_port}}

# Stop the stack.
stop:
    docker compose stop

# Follow Maio's log.
logs:
    docker compose logs -f maio

# Every test: Rust, the web helpers and the engine.
test:
    cargo test
    node --test web/tests/*.test.mjs
    cd engine && uv run pytest -q
