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
  local labels=("$@")
  local intent_labels=()
  local unknown_type_labels=()
  local intent_label="none"
  local should_release="false"
  local bump_level="none"
  local label
  local allowed

  for label in "${labels[@]:-}"; do
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
    warn "unknown intent label(s): $(IFS=','; echo "${unknown_type_labels[*]}")"
    printf 'invalid\nfalse\nnone\n'
    return 0
  fi

  if (( ${#intent_labels[@]} != 1 )); then
    if (( ${#intent_labels[@]} == 0 )); then
      warn "missing intent label (expected exactly one of: $(IFS='|'; echo "${allowed_intent_labels[*]}"))"
    else
      warn "intent labels are mutually exclusive; found: $(IFS=','; echo "${intent_labels[*]}")"
    fi
    printf 'invalid\nfalse\nnone\n'
    return 0
  fi

  intent_label="${intent_labels[0]}"

  case "${intent_label}" in
    type:major)
      should_release="true"
      bump_level="major"
      ;;
    type:minor)
      should_release="true"
      bump_level="minor"
      ;;
    type:patch)
      should_release="true"
      bump_level="patch"
      ;;
    type:docs|type:skip)
      should_release="false"
      bump_level="none"
      ;;
    *)
      should_release="false"
      bump_level="none"
      warn "unexpected intent label: ${intent_label}"
      ;;
  esac

  printf '%s\n%s\n%s\n' "${intent_label}" "${should_release}" "${bump_level}"
}

extract_pr_number_from_commit_subject() {
  local subject="$1"

  if [[ "${subject}" =~ ^Merge\ pull\ request\ \#([0-9]+)\  ]]; then
    printf '%s\n' "${BASH_REMATCH[1]}"
    return 0
  fi

  if [[ "${subject}" =~ ^.+\ \(\#([0-9]+)\)$ ]]; then
    printf '%s\n' "${BASH_REMATCH[1]}"
    return 0
  fi

  return 1
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

if [[ "${ref}" == refs/tags/* ]]; then
  # Tag push:
  # - Allow release-meta to validate tag format and propagate a clear failure for invalid tags.
  # - Avoid duplicate releases when the workflow itself creates/pushes a tag on main.
  if [[ "${GITHUB_ACTOR:-}" == "github-actions[bot]" ]]; then
    should_release="false"
    bump_level="none"
    intent_label="push:tag:bot-skip"
    pr_number="none"
    log "tag push by github-actions[bot]; skip to avoid duplicate release ref=${ref}"
  else
    should_release="true"
    bump_level="none"
    intent_label="push:tag"
    pr_number="none"
    log "tag push ref=${ref} should_release=${should_release} bump_level=${bump_level}"
  fi

  write_output "should_release" "${should_release}"
  write_output "bump_level" "${bump_level}"
  write_output "intent_label" "${intent_label}"
  write_output "pr_number" "${pr_number}"
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

pr_numbers=()
while IFS= read -r pr_number_candidate; do
  [[ -n "${pr_number_candidate}" ]] || continue
  pr_numbers+=("${pr_number_candidate}")
done < <(
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
  # GitHub's commits/{sha}/pulls endpoint may return an empty list for merge commits created by
  # the "Create a merge commit" or squash merge strategy, even though the commit subject still
  # ends with a trustworthy PR marker. Keep this fallback strict: only accept merge subjects or
  # squash subjects that end with ` (#<n>)`.
  commit_subject="$(git log -1 --format=%s "${sha}" 2>/dev/null || true)"
  if fallback_pr_number="$(extract_pr_number_from_commit_subject "${commit_subject}")"; then
    pr_number="${fallback_pr_number}"
    log "no associated PR via commits API for sha=${sha}; using commit-subject fallback pr=${pr_number}"
  else
    log "no associated PR for sha=${sha}; policy: skip auto release"
    write_output "should_release" "false"
    write_output "bump_level" "none"
    write_output "intent_label" "none"
    write_output "pr_number" "none"
    exit 0
  fi
fi

if (( ${#pr_numbers[@]} > 1 )); then
  warn "multiple associated PRs for sha=${sha}: $(IFS=','; echo "${pr_numbers[*]}"); policy: skip auto release"
  write_output "should_release" "false"
  write_output "bump_level" "none"
  write_output "intent_label" "none"
  write_output "pr_number" "none"
  exit 0
fi

if [[ "${pr_number}" == "none" ]]; then
  pr_number="${pr_numbers[0]}"
fi
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

labels=()
while IFS= read -r label; do
  [[ -n "${label}" ]] || continue
  labels+=("${label}")
done < <(
  ISSUE_JSON="${issue_json}" python3 - <<'PY'
import json, os
data = json.loads(os.environ["ISSUE_JSON"])
for l in data.get("labels", []) or []:
  name = l.get("name")
  if name:
    print(name)
PY
)

classification=()
while IFS= read -r classification_value; do
  classification+=("${classification_value}")
done < <(classify_labels "${labels[@]:-}")

intent_label="${classification[0]:-invalid}"
should_release="${classification[1]:-false}"
bump_level="${classification[2]:-none}"

log "sha=${sha} pr=${pr_number} intent_label=${intent_label} should_release=${should_release} bump_level=${bump_level}"

write_output "should_release" "${should_release}"
write_output "bump_level" "${bump_level}"
write_output "intent_label" "${intent_label}"
write_output "pr_number" "${pr_number}"
