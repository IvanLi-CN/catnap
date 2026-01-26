#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

PID_FILE="tmp-catnap-run.pid"
LOG_FILE="tmp-catnap-run.log"
SCREEN_SESSION="catnap-backend"
SCREEN_LOG="out/dev/screen/backend-18090.log"

PORT="${CATNAP_DEV_PORT:-18090}"
BIND_ADDR="0.0.0.0:${PORT}"
URL_ADDR="127.0.0.1:${PORT}"

if lsof -nP -iTCP:"$PORT" -sTCP:LISTEN >/dev/null 2>&1; then
  echo "Port $PORT already in use:"
  lsof -nP -iTCP:"$PORT" -sTCP:LISTEN || true
  echo "Stop the other process or set CATNAP_DEV_PORT to a free port."
  exit 1
fi

if command -v screen >/dev/null 2>&1; then
  if screen -list | grep -q "\\.${SCREEN_SESSION}[[:space:]]"; then
    echo "catnap backend already running (screen=$SCREEN_SESSION) at http://$URL_ADDR/#ops"
    exit 0
  fi

  mkdir -p "$(dirname "$SCREEN_LOG")"
  rm -f "$SCREEN_LOG"

  echo "Starting catnap backend in screen session '$SCREEN_SESSION' at http://$URL_ADDR/#ops"
  echo "Log: $SCREEN_LOG"

  screen -dmS "$SCREEN_SESSION" bash -lc "
    cd '$ROOT_DIR' &&
    if command -v bun >/dev/null 2>&1; then
      cd '$ROOT_DIR/web' &&
      bun run build >/dev/null &&
      cd '$ROOT_DIR' &&
      true
    fi &&
    export BIND_ADDR='$BIND_ADDR' &&
    export CATNAP_DEV_USER_ID='u_1' &&
    export CATNAP_DB_URL='sqlite::memory:' &&
    exec cargo run 2>&1 | tee -a '$SCREEN_LOG'
  "

  for _ in {1..120}; do
    code="$(curl -sS -o /dev/null -w "%{http_code}" "http://$URL_ADDR/healthz" || true)"
    if [[ "$code" == "200" ]]; then
      echo "ready: http://$URL_ADDR/#ops"
      echo "attach: screen -r $SCREEN_SESSION"
      exit 0
    fi
    sleep 0.1
  done

  echo "not ready yet; check logs: tail -f $SCREEN_LOG"
  exit 1
fi

if [[ -f "$PID_FILE" ]]; then
  pid="$(cat "$PID_FILE" || true)"
  if [[ -n "${pid}" ]] && kill -0 "$pid" 2>/dev/null; then
    echo "catnap backend already running (pid=$pid) at http://$URL_ADDR/#ops"
    exit 0
  fi
fi

echo "Starting catnap backend at http://$URL_ADDR/#ops"
echo "Log: $LOG_FILE"

rm -f "$PID_FILE"

nohup bash -lc "
  if command -v bun >/dev/null 2>&1; then
    cd '$ROOT_DIR/web' &&
    bun run build >/dev/null &&
    cd '$ROOT_DIR' &&
    true
  fi &&
  export BIND_ADDR='$BIND_ADDR' &&
  export CATNAP_DEV_USER_ID='u_1' &&
  export CATNAP_DB_URL='sqlite::memory:' &&
  exec cargo run
" >"$LOG_FILE" 2>&1 &

echo $! >"$PID_FILE"

echo "pid=$(cat "$PID_FILE")"
echo "Tip: tail -f $LOG_FILE"

for _ in {1..100}; do
  code="$(curl -sS -o /dev/null -w "%{http_code}" "http://$URL_ADDR/healthz" || true)"
  if [[ "$code" == "200" ]]; then
    echo "ready: http://$URL_ADDR/#ops"
    exit 0
  fi
  sleep 0.1
done

echo "not ready yet; check logs: tail -f $LOG_FILE"
