#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

PID_FILE="tmp-catnap-run.pid"
SCREEN_SESSION="catnap-backend"
PORT="${CATNAP_DEV_PORT:-18090}"

if command -v screen >/dev/null 2>&1; then
  if screen -list | grep -q "\\.${SCREEN_SESSION}[[:space:]]"; then
    echo "Stopping catnap backend screen session '$SCREEN_SESSION'..."
    screen -S "$SCREEN_SESSION" -X quit >/dev/null 2>&1 || true
    sleep 0.2
  fi

  if lsof -nP -iTCP:"$PORT" -sTCP:LISTEN >/dev/null 2>&1; then
    pid="$(lsof -nP -iTCP:"$PORT" -sTCP:LISTEN -t 2>/dev/null | head -n 1 || true)"
    if [[ -n "$pid" ]]; then
      echo "Stopping listener on port $PORT (pid=$pid)..."
      kill "$pid" 2>/dev/null || true
    fi
  fi
fi

if [[ ! -f "$PID_FILE" ]]; then
  echo "No pid file ($PID_FILE)."
  exit 0
fi

pid="$(cat "$PID_FILE" || true)"
if [[ -z "${pid}" ]]; then
  echo "Empty pid file ($PID_FILE)."
  rm -f "$PID_FILE"
  exit 0
fi

if ! kill -0 "$pid" 2>/dev/null; then
  echo "Process not running (pid=$pid)."
  rm -f "$PID_FILE"
  exit 0
fi

echo "Stopping catnap backend (pid=$pid)..."
kill "$pid" 2>/dev/null || true

for _ in {1..30}; do
  if kill -0 "$pid" 2>/dev/null; then
    sleep 0.2
  else
    break
  fi
done

if kill -0 "$pid" 2>/dev/null; then
  echo "Still running; sending SIGKILL..."
  kill -9 "$pid" 2>/dev/null || true
fi

rm -f "$PID_FILE"
echo "Stopped."
