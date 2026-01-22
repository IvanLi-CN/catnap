#!/usr/bin/env bash
set -euo pipefail

compose_file="${1:-deploy/compose.yaml}"
base_url="${2:-http://127.0.0.1:8080}"

compose_cmd=(docker compose)
if ! docker compose version >/dev/null 2>&1; then
  compose_cmd=(docker-compose)
fi

if [[ ! -f "${compose_file}" ]]; then
  echo "compose file not found: ${compose_file}" >&2
  exit 1
fi

compose_dir="$(cd "$(dirname "${compose_file}")" && pwd -P)"
compose_file_basename="$(basename "${compose_file}")"

cleanup() {
  (cd "${compose_dir}" && "${compose_cmd[@]}" -f "${compose_file_basename}" down -v --remove-orphans) >/dev/null 2>&1 || true
}
trap cleanup EXIT

if [[ -f "${compose_dir}/.env.example" ]] && [[ ! -f "${compose_dir}/.env" ]]; then
  cp "${compose_dir}/.env.example" "${compose_dir}/.env"
fi

(
  cd "${compose_dir}"
  "${compose_cmd[@]}" -f "${compose_file_basename}" up -d --no-build
)

wait_ready() {
  for _ in $(seq 1 120); do
    if curl -fsS "${base_url}/healthz" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.5
  done
  return 1
}

if ! wait_ready; then
  echo "compose stack failed to become ready: ${base_url}" >&2
  echo "--- docker compose ps ---" >&2
  (cd "${compose_dir}" && "${compose_cmd[@]}" -f "${compose_file_basename}" ps) >&2 || true
  echo "--- docker compose logs (tail) ---" >&2
  (cd "${compose_dir}" && "${compose_cmd[@]}" -f "${compose_file_basename}" logs --tail 200) >&2 || true
  exit 1
fi

catnap_container_id="$(
  cd "${compose_dir}"
  "${compose_cmd[@]}" -f "${compose_file_basename}" ps -q catnap
)"
if [[ -z "${catnap_container_id}" ]]; then
  echo "failed to resolve catnap container id" >&2
  (cd "${compose_dir}" && "${compose_cmd[@]}" -f "${compose_file_basename}" ps) >&2 || true
  exit 1
fi

compose_network="$(
  docker inspect -f '{{range $name, $_ := .NetworkSettings.Networks}}{{println $name}}{{end}}' "${catnap_container_id}" \
    | head -n 1 \
    | tr -d '\r'
)"
if [[ -z "${compose_network}" ]]; then
  echo "failed to resolve compose network for container: ${catnap_container_id}" >&2
  docker inspect "${catnap_container_id}" >&2 || true
  exit 1
fi

curl_code_in_network() {
  local url="$1"
  docker run --rm --network "${compose_network}" curlimages/curl:8.5.0 \
    -sS -o /dev/null -w '%{http_code}' \
    "${url}"
}

# Negative checks: direct access to backend should be unauthorized without injected user header.
api_unauth_code="$(curl_code_in_network "http://catnap:18080/api/health")"
if [[ "${api_unauth_code}" != "401" ]]; then
  echo "expected 401 from direct backend /api/health, got ${api_unauth_code}" >&2
  exit 1
fi

ui_unauth_code="$(curl_code_in_network "http://catnap:18080/")"
if [[ "${ui_unauth_code}" != "401" ]]; then
  echo "expected 401 from direct backend /, got ${ui_unauth_code}" >&2
  exit 1
fi

api_json="$(
  origin="$(
    printf '%s' "${base_url}" | sed -E 's#^(https?://[^/]+).*#\\1#'
  )"
  curl -fsS \
    -H "Origin: ${origin}" \
    "${base_url}/api/health"
)"

API_JSON="${api_json}" python3 - <<'PY'
import json
import os
import sys

expected = os.environ.get("APP_EFFECTIVE_VERSION", "").strip()
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
  curl -fsSI "${base_url}/"
)"

echo "${ui_head}" | grep -qi '^content-type:.*text/html' || {
  echo "unexpected Content-Type for /" >&2
  echo "${ui_head}" >&2
  exit 1
}

ui_body="$(
  curl -fsS "${base_url}/"
)"

echo "${ui_body}" | grep -qi '<!doctype html' || {
  echo "missing <!doctype html> in / response" >&2
  exit 1
}
