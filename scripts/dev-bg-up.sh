#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
session="catnap-dev"
out_dir="$root_dir/out/dev/screen"

mkdir -p "$out_dir"

if ! command -v screen >/dev/null 2>&1; then
  echo "screen not found; use scripts/dev-up.sh instead." >&2
  exit 1
fi

if ! command -v bun >/dev/null 2>&1; then
  echo "bun not found in PATH; install bun first." >&2
  exit 1
fi

backend_port="${CATNAP_DEV_PORT:-18090}"
backend_bind_addr="0.0.0.0:${backend_port}"
backend_url_addr="127.0.0.1:${backend_port}"
storybook_port="${CATNAP_STORYBOOK_PORT:-18181}"
web_dev_port="${CATNAP_WEB_DEV_PORT:-5173}"

assert_port_free() {
  local port="$1"
  if lsof -nP -iTCP:"$port" -sTCP:LISTEN >/dev/null 2>&1; then
    echo "Port $port already in use:" >&2
    lsof -nP -iTCP:"$port" -sTCP:LISTEN >&2 || true
    exit 1
  fi
}

assert_port_free "$backend_port"
assert_port_free "$storybook_port"
assert_port_free "$web_dev_port"

echo "building embedded web dist..."
(cd "$root_dir/web" && bun run build >/dev/null)

if screen -list | grep -q "\\.${session}[[:space:]]"; then
  echo "screen session '$session' already running"
  echo "attach: screen -r $session"
  exit 0
fi

start_window() {
  local title="$1"
  local logfile="$2"
  shift 2

  # Use bash -lc to inherit user PATH and allow env exports.
  screen -S "$session" -X screen -t "$title" bash -lc "$* 2>&1 | tee -a '$logfile'"
}

echo "starting screen session '$session'..."
screen -dmS "$session" -t shell bash -lc "echo 'catnap dev session'; exec bash"

start_window "backend" "$out_dir/backend.log" \
  "cd '$root_dir' && CATNAP_AUTH_USER_HEADER='x-user' CATNAP_DEV_USER_ID='u_1' CATNAP_DB_URL='sqlite::memory:' BIND_ADDR='$backend_bind_addr' exec cargo run"

start_window "web" "$out_dir/web.log" \
  "cd '$root_dir/web' && export API_PROXY_TARGET='http://$backend_url_addr' API_PROXY_USER_HEADER='x-user' API_PROXY_USER='u_1'; exec bun run dev -- --host 0.0.0.0 --port $web_dev_port --strictPort"

start_window "storybook" "$out_dir/storybook.log" \
  "cd '$root_dir/web' && exec bun run storybook:ci -- --port $storybook_port"

cat <<EOF

ready (screen):
- backend:   http://$backend_url_addr/#ops
- storybook: http://127.0.0.1:$storybook_port/
- web dev:   http://127.0.0.1:$web_dev_port/

attach: screen -r $session
logs: $out_dir/*.log
stop: scripts/dev-bg-down.sh
EOF
