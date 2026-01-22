#!/usr/bin/env bash
set -euo pipefail

log() {
  echo "ci-path-gate: $*" >&2
}

warn() {
  echo "ci-path-gate: WARN: $*" >&2
}

json_get() {
  local key="$1"
  local path="${GITHUB_EVENT_PATH:-}"
  if [[ -z "${path}" || ! -f "${path}" ]]; then
    return 0
  fi
  python3 - <<'PY' "${path}" "${key}" || true
import json
import sys

path = sys.argv[1]
key = sys.argv[2]

try:
  data = json.load(open(path, "r", encoding="utf-8"))
except Exception:
  sys.exit(0)

def dig(obj, keys):
  cur = obj
  for k in keys:
    if not isinstance(cur, dict):
      return ""
    cur = cur.get(k)
  if isinstance(cur, str):
    return cur
  return ""

if key == "pr.base.sha":
  print(dig(data, ["pull_request", "base", "sha"]))
elif key == "pr.head.sha":
  print(dig(data, ["pull_request", "head", "sha"]))
elif key == "push.before":
  print(dig(data, ["before"]))
elif key == "push.after":
  print(dig(data, ["after"]))
else:
  sys.exit(0)
PY
}

ci_base_sha="${CI_BASE_SHA:-}"
ci_head_sha="${CI_HEAD_SHA:-}"
ci_assume_changed="${CI_ASSUME_CHANGED:-true}"

if [[ -z "${ci_base_sha}" ]]; then
  ci_base_sha="$(json_get pr.base.sha)"
  if [[ -z "${ci_base_sha}" ]]; then
    ci_base_sha="$(json_get push.before)"
  fi
fi

if [[ -z "${ci_head_sha}" ]]; then
  ci_head_sha="$(json_get pr.head.sha)"
  if [[ -z "${ci_head_sha}" ]]; then
    ci_head_sha="$(json_get push.after)"
  fi
fi

# Tag create pushes have a synthetic all-zero "before" sha; treat it as absent so we can fall back to merge-base.
if [[ "${ci_base_sha:-}" =~ ^0{40}$ ]]; then
  ci_base_sha=""
fi

if [[ -z "${ci_head_sha}" ]]; then
  ci_head_sha="$(git rev-parse HEAD)"
fi

if [[ -z "${ci_base_sha}" ]]; then
  if git rev-parse -q --verify origin/main >/dev/null; then
    ci_base_sha="$(git merge-base origin/main "${ci_head_sha}" || true)"
  fi
fi

if [[ -z "${ci_base_sha}" ]]; then
  ci_base_sha="$(git rev-parse "${ci_head_sha}^" 2>/dev/null || true)"
fi

reason=""
files=""
diff_computed="false"
if [[ -z "${ci_base_sha}" ]]; then
  reason="no_base_sha"
else
  if git cat-file -e "${ci_base_sha}^{commit}" 2>/dev/null && git cat-file -e "${ci_head_sha}^{commit}" 2>/dev/null; then
    files="$(git diff --name-only "${ci_base_sha}...${ci_head_sha}" || true)"
    reason="diff:${ci_base_sha:0:7}...${ci_head_sha:0:7}"
    diff_computed="true"
  else
    warn "missing commit objects for diff: base=${ci_base_sha} head=${ci_head_sha}"
    reason="missing_git_objects"
  fi
fi

frontend_changed="false"
backend_changed="false"
docker_changed="false"

if [[ "${diff_computed}" == "true" ]]; then
  if [[ -n "${files}" ]]; then
    while IFS= read -r f; do
      [[ -z "${f}" ]] && continue

      case "${f}" in
        web/*) frontend_changed="true" ;;
      esac

      case "${f}" in
        src/*|Cargo.toml|Cargo.lock) backend_changed="true" ;;
      esac

      case "${f}" in
        Dockerfile|.github/*|deploy/*) docker_changed="true" ;;
      esac
    done <<<"${files}"
  else
    reason="${reason};no_changes"
  fi
else
  if [[ "${ci_assume_changed}" == "true" ]]; then
    frontend_changed="true"
    backend_changed="true"
    docker_changed="true"
    reason="${reason};assume_changed=true"
  else
    reason="${reason};assume_changed=false"
  fi
fi

out="${GITHUB_OUTPUT:-/dev/stdout}"
{
  echo "frontend_changed=${frontend_changed}"
  echo "backend_changed=${backend_changed}"
  echo "docker_changed=${docker_changed}"
  echo "reason=${reason}"
} >> "${out}"

log "frontend_changed=${frontend_changed} backend_changed=${backend_changed} docker_changed=${docker_changed} reason=${reason}"
