#!/usr/bin/env bash
set -euo pipefail

output_file="${GITHUB_OUTPUT:-}"

log() {
  echo "release-intent: $*"
}

warn() {
  echo "release-intent: WARN: $*" >&2
}

fail() {
  echo "release-intent: $*" >&2
  exit 1
}

write_output() {
  local key="$1"
  local value="$2"

  if [[ -n "${output_file}" ]]; then
    echo "${key}=${value}" >> "${output_file}"
  else
    echo "${key}=${value}"
  fi
}

write_false_outputs() {
  write_output "should_release" "false"
  write_output "bump_level" "none"
  write_output "intent_label" "none"
  write_output "channel_label" "none"
  write_output "pr_number" "none"
  write_output "resolution_source" "none"
  write_output "channel_source" "none"
}

event_name="${GITHUB_EVENT_NAME:-}"
ref="${GITHUB_REF:-}"
ref_name="${GITHUB_REF_NAME:-}"
repo="${GITHUB_REPOSITORY:-}"
sha="${GITHUB_SHA:-}"
repo_root="${GITHUB_WORKSPACE:-.}"
legacy_missing_channel="${RELEASE_LEGACY_MISSING_CHANNEL:-stable}"

if [[ -z "${repo}" ]]; then
  fail "GITHUB_REPOSITORY is required"
fi

if [[ "${event_name}" == "workflow_dispatch" ]]; then
  if [[ "${ref}" == refs/tags/* ]]; then
    tag="${ref_name}"
    if [[ "${tag}" != v* ]]; then
      fail "tag must start with 'v': ${tag}"
    fi
    write_output "should_release" "true"
    write_output "bump_level" "none"
    write_output "intent_label" "manual:tag"
    write_output "channel_label" "channel:stable"
    write_output "pr_number" "none"
    write_output "resolution_source" "manual:tag"
    write_output "channel_source" "manual"
    log "manual publish ref=tag tag=${tag} should_release=true"
    exit 0
  fi

  if [[ "${ref}" == "refs/heads/main" ]]; then
    bump_level="${BUMP_LEVEL:-}"
    if [[ -z "${bump_level}" ]]; then
      fail "workflow_dispatch ref=main requires input bump_level (major|minor|patch)"
    fi
    case "${bump_level}" in
      major|minor|patch) ;;
      *) fail "invalid bump_level: ${bump_level} (expected major|minor|patch)" ;;
    esac
    write_output "should_release" "true"
    write_output "bump_level" "${bump_level}"
    write_output "intent_label" "manual:${bump_level}"
    write_output "channel_label" "channel:stable"
    write_output "pr_number" "none"
    write_output "resolution_source" "manual:main"
    write_output "channel_source" "manual"
    log "manual publish ref=main bump_level=${bump_level} should_release=true"
    exit 0
  fi

  warn "manual dispatch on non-release ref ${ref}; defaulting to should_release=false"
  write_false_outputs
  exit 0
fi

if [[ "${event_name}" != "push" ]]; then
  warn "unsupported event_name for automatic release intent: ${event_name}; defaulting to should_release=false"
  write_false_outputs
  exit 0
fi

if [[ "${ref}" == refs/tags/* ]]; then
  if [[ "${GITHUB_ACTOR:-}" == "github-actions[bot]" ]]; then
    write_output "should_release" "false"
    write_output "bump_level" "none"
    write_output "intent_label" "push:tag:bot-skip"
    write_output "channel_label" "channel:stable"
    write_output "pr_number" "none"
    write_output "resolution_source" "tag_push_bot_skip"
    write_output "channel_source" "push"
    log "tag push by github-actions[bot]; skip to avoid duplicate release ref=${ref}"
    exit 0
  fi

  write_output "should_release" "true"
  write_output "bump_level" "none"
  write_output "intent_label" "push:tag"
  write_output "channel_label" "channel:stable"
  write_output "pr_number" "none"
  write_output "resolution_source" "tag_push"
  write_output "channel_source" "push"
  log "tag push ref=${ref} should_release=true"
  exit 0
fi

if [[ "${ref}" != "refs/heads/main" ]]; then
  warn "unsupported ref for automatic release intent: ${ref}; defaulting to should_release=false"
  write_false_outputs
  exit 0
fi

if [[ -z "${sha}" ]]; then
  fail "GITHUB_SHA is required"
fi

if [[ -z "${GITHUB_TOKEN:-}" ]]; then
  fail "GITHUB_TOKEN is required"
fi

if ! python3 ./.github/scripts/release_plan.py inspect-commit \
  --repo "${repo}" \
  --repo-root "${repo_root}" \
  --target-sha "${sha}" \
  --legacy-missing-channel "${legacy_missing_channel}" \
  --github-output "${output_file}"; then
  warn "failed to resolve automatic release intent for sha=${sha}; defaulting to should_release=false"
  write_false_outputs
fi
