#!/usr/bin/env bash
# Runs a fake engine and Maio on a fresh database with a few songs, for looking at the web UI.
# Usage: [EMPTY=1] scripts/ui-preview.sh [port]   (default 8099). Prints the session cookie; Ctrl-C stops both.
set -euo pipefail
PORT="${1:-8099}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DATA="$(mktemp -d)"
export MAIO_ADMIN_USER=admin MAIO_ADMIN_PASSWORD=preview-password T_FAKE=dev
cat > "$DATA/maio.toml" <<EOF
listen = "127.0.0.1:$PORT"
data_dir = "$DATA"
moods = ["calm", "focused", "upbeat", "tense", "dreamy"]
genres = ["lo-fi", "ambient", "synthwave", "jazz"]
crossfade_s = 1.0
[serve]
mp3 = true
[[engine]]
id = "fake"
url = "http://127.0.0.1:7899"
token_env = "T_FAKE"
EOF
(cd "${ENGINE_ROOT:-$ROOT/../maio-engine}" && uv run maio-engine --backend fake --port 7899 >/dev/null 2>&1) &
ENGINE=$!
(cd "$ROOT" && cargo run -q -- "$DATA/maio.toml" >"$DATA/maio.log" 2>&1) &
APP=$!
trap 'kill $ENGINE $APP 2>/dev/null; rm -rf "$DATA"' EXIT
for _ in $(seq 1 120); do curl -sf "http://127.0.0.1:$PORT/" >/dev/null && break; sleep 1; done
O="Origin: http://127.0.0.1:$PORT"
COOKIE=$(curl -si -H "$O" -H 'Content-Type: application/json' -d '{"username":"admin","password":"preview-password","remember":true}' "http://127.0.0.1:$PORT/api/v1/session" | tr -d '\r' | sed -n 's/^set-cookie: \(maio_session=[^;]*\).*/\1/ip')
C="Cookie: $COOKIE"
J='Content-Type: application/json'
if [ -z "${EMPTY:-}" ]; then  # EMPTY=1: a fresh server with nothing in it
  for m in calm dreamy tense; do curl -sf -H "$O" -H "$C" -H "$J" -d "{\"request\":{\"mood\":\"$m\",\"seconds\":20}}" "http://127.0.0.1:$PORT/api/v1/queue" >/dev/null; done
  curl -sf -H "$O" -H "$C" -H "$J" -d '{"name":"Late night coding","kind":"generative","recipe":{"prompt":"lo-fi, warm Rhodes, soft rain","mood":"calm","seconds":[20,30],"library_share":0.25}}' "http://127.0.0.1:$PORT/api/v1/playlists" >/dev/null
  curl -sf -H "$O" -H "$C" -H "$J" -d '{"id":"lobby","name":"Lobby"}' "http://127.0.0.1:$PORT/api/v1/radios" >/dev/null
fi
echo "http://127.0.0.1:$PORT"
echo "$COOKIE"
wait $APP
