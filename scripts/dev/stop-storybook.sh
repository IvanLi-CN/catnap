#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

PID_FILE="tmp-storybook-run.pid"

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

echo "Stopping storybook (pid=$pid)..."
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
