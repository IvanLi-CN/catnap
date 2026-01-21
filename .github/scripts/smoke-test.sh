#!/usr/bin/env bash
set -euo pipefail

bin_path="${1:-./target/release/catnap}"

if [[ ! -x "${bin_path}" ]]; then
  echo "binary not found or not executable: ${bin_path}" >&2
  exit 1
fi

bind_addr="${BIND_ADDR:-127.0.0.1:0}"
export BIND_ADDR="${bind_addr}"
export CATNAP_DB_URL="${CATNAP_DB_URL:-sqlite::memory:}"
export CATNAP_AUTH_USER_HEADER="${CATNAP_AUTH_USER_HEADER:-x-user}"
export APP_EFFECTIVE_VERSION="${APP_EFFECTIVE_VERSION:-0.0.0}"

tmp_log="$(mktemp)"
"${bin_path}" >"${tmp_log}" 2>&1 &
pid="$!"

cleanup() {
  kill "${pid}" >/dev/null 2>&1 || true
  wait "${pid}" >/dev/null 2>&1 || true
  rm -f "${tmp_log}"
}
trap cleanup EXIT

strip_ansi() {
  sed -E $'s/\x1B\\[[0-9;]*[a-zA-Z]//g'
}

wait_ready() {
  local addr port base_url
  for _ in $(seq 1 120); do
    addr="$(
      grep -m 1 -F "listening" "${tmp_log}" 2>/dev/null | strip_ansi | sed -E 's/.*addr=([^ ]+).*/\1/' || true
    )"
    if [[ -n "${addr}" ]] && [[ "${addr}" =~ ^[0-9.]+:[0-9]+$ ]]; then
      port="${addr##*:}"
      base_url="http://127.0.0.1:${port}"
      if curl -fsS "${base_url}/healthz" >/dev/null 2>&1; then
        echo "${base_url}"
        return 0
      fi
    fi
    sleep 0.25
  done
  return 1
}

base_url="$(wait_ready || true)"
if [[ -z "${base_url}" ]]; then
  echo "server failed to become ready (BIND_ADDR=${BIND_ADDR})" >&2
  echo "--- server log ---" >&2
  tail -n 200 "${tmp_log}" >&2 || true
  exit 1
fi

api_json="$(
  curl -fsS \
    -H 'host: example.com' \
    -H 'origin: http://example.com' \
    -H 'x-user: smoke' \
    "${base_url}/api/health"
)"

API_JSON="${api_json}" python3 - <<'PY'
import json
import os
import sys

expected = os.environ.get("APP_EFFECTIVE_VERSION", "")
payload = json.loads(os.environ["API_JSON"])

status = payload.get("status")
version = payload.get("version")

if status != "ok":
    print(f"unexpected status: {status}", file=sys.stderr)
    sys.exit(1)

if not version:
    print("missing version in /api/health response", file=sys.stderr)
    sys.exit(1)

if expected and version != expected:
    print(f"version mismatch: expected={expected} got={version}", file=sys.stderr)
    sys.exit(1)
PY

ui_head="$(
  curl -fsSI \
    -H 'host: example.com' \
    -H 'x-user: smoke' \
    "${base_url}/"
)"

echo "${ui_head}" | grep -qi '^content-type:.*text/html' || {
  echo "unexpected Content-Type for /" >&2
  echo "${ui_head}" >&2
  exit 1
}

ui_body="$(
  curl -fsS \
    -H 'host: example.com' \
    -H 'x-user: smoke' \
    "${base_url}/"
)"

echo "${ui_body}" | grep -qi '<!doctype html' || {
  echo "missing <!doctype html> in / response" >&2
  exit 1
}
