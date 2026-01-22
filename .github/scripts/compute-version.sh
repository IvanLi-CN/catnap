#!/usr/bin/env bash
set -euo pipefail

# Compute effective semver from git tags (fallback Cargo.toml), with bump level and uniqueness.

print_version="false"
github_output_path=""

# Important: this script is sometimes sourced. When sourced, do NOT parse caller's positional args.
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --print-version)
        print_version="true"
        shift
        ;;
      --github-output)
        if [[ $# -lt 2 ]]; then
          echo "Missing value for --github-output" >&2
          exit 2
        fi
        github_output_path="$2"
        shift 2
        ;;
      *)
        echo "Unknown argument: $1" >&2
        exit 2
        ;;
    esac
  done
fi

log() {
  if [[ "${print_version}" == "true" ]]; then
    echo "$@" >&2
  else
    echo "$@"
  fi
}

root_dir="$(git rev-parse --show-toplevel)"

git fetch --tags --force >/dev/null 2>&1 || true

bump_level="${BUMP_LEVEL:-}"
if [[ -z "${bump_level}" ]]; then
  echo "BUMP_LEVEL must be set to one of: major|minor|patch" >&2
  exit 1
fi

case "${bump_level}" in
  major|minor|patch) ;;
  *)
    echo "Invalid BUMP_LEVEL: ${bump_level}. Expected: major|minor|patch" >&2
    exit 1
    ;;
esac

cargo_ver="$(
  grep -m1 '^version[[:space:]]*=[[:space:]]*"' "$root_dir/Cargo.toml" \
    | sed -E 's/.*"([0-9]+\.[0-9]+\.[0-9]+)".*/\1/'
)"

if [[ -z "${cargo_ver:-}" ]]; then
  echo "Failed to detect version from Cargo.toml" >&2
  exit 1
fi

latest_tag="$(
  git tag -l \
    | grep -E '^v[0-9]+\\.[0-9]+\\.[0-9]+$' \
    | sort -V \
    | tail -n1 || true
)"

if [[ -n "${latest_tag}" ]]; then
  base_ver="${latest_tag#v}"
  base_source="tag ${latest_tag}"
else
  base_ver="${cargo_ver}"
  base_source="Cargo.toml ${cargo_ver}"
fi

base_major="$(echo "$base_ver" | cut -d. -f1)"
base_minor="$(echo "$base_ver" | cut -d. -f2)"
base_patch="$(echo "$base_ver" | cut -d. -f3)"

next_major="${base_major}"
next_minor="${base_minor}"
next_patch="${base_patch}"

case "${bump_level}" in
  major)
    next_major="$((base_major + 1))"
    next_minor="0"
    next_patch="0"
    ;;
  minor)
    next_minor="$((base_minor + 1))"
    next_patch="0"
    ;;
  patch)
    next_patch="$((base_patch + 1))"
    ;;
esac

candidate="${next_patch}"
while git rev-parse -q --verify "refs/tags/v${next_major}.${next_minor}.${candidate}" >/dev/null; do
  candidate="$((candidate + 1))"
done

effective="${next_major}.${next_minor}.${candidate}"

export APP_EFFECTIVE_VERSION="${effective}"

if [[ -n "${github_output_path}" ]]; then
  echo "version=${effective}" >> "${github_output_path}"
  echo "APP_EFFECTIVE_VERSION=${effective}" >> "${github_output_path}"
fi

if [[ "${print_version}" == "true" ]]; then
  printf '%s\n' "${effective}"
  exit 0
fi

if [[ -n "${GITHUB_ENV:-}" ]]; then
  echo "APP_EFFECTIVE_VERSION=${effective}" >> "${GITHUB_ENV}"
else
  echo "APP_EFFECTIVE_VERSION=${effective}"
fi

log "Computed APP_EFFECTIVE_VERSION=${effective}"
log "  bump_level=${bump_level}"
log "  base_version=${base_ver} (${base_source})"
log "  target_tag=v${effective}"
