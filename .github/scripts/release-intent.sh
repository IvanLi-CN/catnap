#!/usr/bin/env bash
set -euo pipefail

allowed_intent_labels=(
  "type:docs"
  "type:skip"
  "type:patch"
  "type:minor"
  "type:major"
)

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

classify_labels() {
  local -n _labels_ref="$1"
  local -n _intent_label_ref="$2"
  local -n _should_release_ref="$3"
  local -n _bump_level_ref="$4"

  local intent_labels=()
  local unknown_type_labels=()

  for label in "${_labels_ref[@]:-}"; do
    if [[ "${label}" == type:* ]]; then
      local is_allowed="false"
      for allowed in "${allowed_intent_labels[@]}"; do
        if [[ "${label}" == "${allowed}" ]]; then
          is_allowed="true"
          break
        fi
      done

      if [[ "${is_allowed}" == "true" ]]; then
        intent_labels+=("${label}")
      else
        unknown_type_labels+=("${label}")
      fi
    fi
  done

  if (( ${#unknown_type_labels[@]} > 0 )); then
    _intent_label_ref="invalid"
    _should_release_ref="false"
    _bump_level_ref="none"
    warn "unknown intent label(s): $(IFS=','; echo "${unknown_type_labels[*]}")"
    return 0
  fi

  if (( ${#intent_labels[@]} != 1 )); then
    _intent_label_ref="invalid"
    _should_release_ref="false"
    _bump_level_ref="none"
    if (( ${#intent_labels[@]} == 0 )); then
      warn "missing intent label (expected exactly one of: $(IFS='|'; echo "${allowed_intent_labels[*]}"))"
    else
      warn "intent labels are mutually exclusive; found: $(IFS=','; echo "${intent_labels[*]}")"
    fi
    return 0
  fi

  _intent_label_ref="${intent_labels[0]}"

  case "${_intent_label_ref}" in
    type:major)
      _should_release_ref="true"
      _bump_level_ref="major"
      ;;
    type:minor)
      _should_release_ref="true"
      _bump_level_ref="minor"
      ;;
    type:patch)
      _should_release_ref="true"
      _bump_level_ref="patch"
      ;;
    type:docs|type:skip)
      _should_release_ref="false"
      _bump_level_ref="none"
      ;;
    *)
      _should_release_ref="false"
      _bump_level_ref="none"
      warn "unexpected intent label: ${_intent_label_ref}"
      ;;
  esac
}

token="${GITHUB_TOKEN:-}"
repo="${GITHUB_REPOSITORY:-}"
sha="${GITHUB_SHA:-}"
event_name="${GITHUB_EVENT_NAME:-}"
ref="${GITHUB_REF:-}"
ref_name="${GITHUB_REF_NAME:-}"

if [[ -z "${token}" ]]; then
  fail "GITHUB_TOKEN is required"
fi
if [[ -z "${repo}" ]]; then
  fail "GITHUB_REPOSITORY is required"
fi
if [[ -z "${sha}" ]]; then
  fail "GITHUB_SHA is required"
fi

api_base="${GITHUB_API_URL:-https://api.github.com}"

should_release="false"
bump_level="none"
intent_label="none"
pr_number="none"

if [[ "${event_name}" == "workflow_dispatch" ]]; then
  if [[ "${ref}" == refs/tags/* ]]; then
    tag="${ref_name}"
    if [[ "${tag}" != v* ]]; then
      fail "tag must start with 'v': ${tag}"
    fi
    should_release="true"
    bump_level="none"
    intent_label="manual:tag"
    pr_number="none"
    log "manual publish ref=tag tag=${tag} should_release=${should_release} bump_level=${bump_level}"
  else
    if [[ "${ref}" != "refs/heads/main" ]]; then
      fail "unsupported ref for manual publish: ${ref}"
    fi

    bump_level="${BUMP_LEVEL:-}"
    if [[ -z "${bump_level}" ]]; then
      fail "workflow_dispatch ref=main requires input bump_level (major|minor|patch)"
    fi
    case "${bump_level}" in
      major|minor|patch) ;;
      *) fail "invalid bump_level: ${bump_level} (expected major|minor|patch)" ;;
    esac

    should_release="true"
    intent_label="manual:${bump_level}"
    pr_number="none"
    log "manual publish ref=main should_release=${should_release} bump_level=${bump_level}"
  fi

  write_output "should_release" "${should_release}"
  write_output "bump_level" "${bump_level}"
  write_output "intent_label" "${intent_label}"
  write_output "pr_number" "${pr_number}"
  exit 0
fi

if [[ "${event_name}" != "push" ]]; then
  warn "unsupported event_name for automatic release intent: ${event_name}; defaulting to should_release=false"
  write_output "should_release" "false"
  write_output "bump_level" "none"
  write_output "intent_label" "none"
  write_output "pr_number" "none"
  exit 0
fi

if [[ "${ref}" != "refs/heads/main" ]]; then
  warn "unsupported ref for automatic release intent: ${ref}; defaulting to should_release=false"
  write_output "should_release" "false"
  write_output "bump_level" "none"
  write_output "intent_label" "none"
  write_output "pr_number" "none"
  exit 0
fi

pulls_url="${api_base}/repos/${repo}/commits/${sha}/pulls"
pulls_json=""

if ! pulls_json="$(
  curl -fsSL \
    -H "Authorization: Bearer ${token}" \
    -H "Accept: application/vnd.github+json" \
    -H "X-GitHub-Api-Version: 2022-11-28" \
    "${pulls_url}"
)"; then
  if ! pulls_json="$(
    curl -fsSL \
      -H "Authorization: Bearer ${token}" \
      -H "Accept: application/vnd.github.groot-preview+json" \
      -H "X-GitHub-Api-Version: 2022-11-28" \
      "${pulls_url}"
  )"; then
    warn "failed to resolve associated PRs for sha=${sha}; defaulting to should_release=false"
    write_output "should_release" "false"
    write_output "bump_level" "none"
    write_output "intent_label" "none"
    write_output "pr_number" "none"
    exit 0
  fi
fi

mapfile -t pr_numbers < <(
  PULLS_JSON="${pulls_json}" python3 - <<'PY'
import json, os
data = json.loads(os.environ["PULLS_JSON"])
for pr in data or []:
  n = pr.get("number")
  if n is not None:
    print(n)
PY
)

if (( ${#pr_numbers[@]} == 0 )); then
  log "no associated PR for sha=${sha}; policy: skip auto release"
  write_output "should_release" "false"
  write_output "bump_level" "none"
  write_output "intent_label" "none"
  write_output "pr_number" "none"
  exit 0
fi

if (( ${#pr_numbers[@]} > 1 )); then
  warn "multiple associated PRs for sha=${sha}: $(IFS=','; echo "${pr_numbers[*]}"); policy: skip auto release"
  write_output "should_release" "false"
  write_output "bump_level" "none"
  write_output "intent_label" "none"
  write_output "pr_number" "none"
  exit 0
fi

pr_number="${pr_numbers[0]}"
issue_url="${api_base}/repos/${repo}/issues/${pr_number}"

issue_json=""
if ! issue_json="$(
  curl -fsSL \
    -H "Authorization: Bearer ${token}" \
    -H "Accept: application/vnd.github+json" \
    -H "X-GitHub-Api-Version: 2022-11-28" \
    "${issue_url}"
)"; then
  warn "failed to fetch PR labels for pr=${pr_number}; policy: skip auto release"
  write_output "should_release" "false"
  write_output "bump_level" "none"
  write_output "intent_label" "none"
  write_output "pr_number" "${pr_number}"
  exit 0
fi

mapfile -t labels < <(
  ISSUE_JSON="${issue_json}" python3 - <<'PY'
import json, os
data = json.loads(os.environ["ISSUE_JSON"])
for l in data.get("labels", []) or []:
  name = l.get("name")
  if name:
    print(name)
PY
)

classify_labels labels intent_label should_release bump_level

log "sha=${sha} pr=${pr_number} intent_label=${intent_label} should_release=${should_release} bump_level=${bump_level}"

write_output "should_release" "${should_release}"
write_output "bump_level" "${bump_level}"
write_output "intent_label" "${intent_label}"
write_output "pr_number" "${pr_number}"
