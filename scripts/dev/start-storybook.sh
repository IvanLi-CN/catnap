#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR/web"

PID_FILE="../tmp-storybook-run.pid"
LOG_FILE="../tmp-storybook-run.log"
SCREEN_SESSION="catnap-storybook"
SCREEN_LOG="../out/dev/screen/storybook-18181.log"

PORT="${CATNAP_STORYBOOK_PORT:-18181}"

if lsof -nP -iTCP:"$PORT" -sTCP:LISTEN >/dev/null 2>&1; then
  if command -v screen >/dev/null 2>&1 && screen -list | grep -q "\\.${SCREEN_SESSION}[[:space:]]"; then
    echo "storybook already running (screen=$SCREEN_SESSION) at http://127.0.0.1:$PORT/"
    exit 0
  fi

  echo "Port $PORT already in use:"
  lsof -nP -iTCP:"$PORT" -sTCP:LISTEN || true
  echo "If it's an old storybook, stop it first."
  exit 1
fi

if [[ -f "$PID_FILE" ]]; then
  pid="$(cat "$PID_FILE" || true)"
  if [[ -n "${pid}" ]] && kill -0 "$pid" 2>/dev/null; then
    echo "storybook already running (pid=$pid) at http://127.0.0.1:$PORT/"
    exit 0
  fi
fi

echo "Starting storybook at http://127.0.0.1:$PORT/"
echo "Log: $LOG_FILE"

if command -v screen >/dev/null 2>&1; then
  mkdir -p "$(dirname "$SCREEN_LOG")"
  rm -f "$SCREEN_LOG"

  rm -f "$PID_FILE"

  echo "Starting storybook in screen session '$SCREEN_SESSION'..."
  echo "Screen log: $SCREEN_LOG"

  screen -dmS "$SCREEN_SESSION" bash -lc "
    cd '$ROOT_DIR/web' &&
    exec bun run storybook:ci -- --port '$PORT' 2>&1 | tee -a '$SCREEN_LOG'
  "

  for _ in {1..120}; do
    if lsof -nP -iTCP:"$PORT" -sTCP:LISTEN >/dev/null 2>&1; then
      echo "ready: http://127.0.0.1:$PORT/"
      echo "attach: screen -r $SCREEN_SESSION"
      exit 0
    fi
    sleep 0.1
  done

  echo "not ready yet; check logs: tail -f $SCREEN_LOG"
  exit 1
fi

rm -f "$PID_FILE"

nohup bash -lc "
  cd '$ROOT_DIR/web' &&
  exec bun run storybook:ci -- --port '$PORT'
" >"$LOG_FILE" 2>&1 &

echo $! >"$PID_FILE"
echo "pid=$(cat "$PID_FILE")"
echo "Tip: tail -f $LOG_FILE"
