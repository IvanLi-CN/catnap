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

fail() {
  echo "label-gate: $*" >&2
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

token="${GITHUB_TOKEN:-}"
repo="${GITHUB_REPOSITORY:-}"
pr_number="${PR_NUMBER:-}"

if [[ -z "${token}" ]]; then
  fail "GITHUB_TOKEN is required"
fi
if [[ -z "${repo}" ]]; then
  fail "GITHUB_REPOSITORY is required"
fi
if [[ -z "${pr_number}" ]]; then
  fail "PR_NUMBER is required"
fi

api_base="${GITHUB_API_URL:-https://api.github.com}"
issue_url="${api_base}/repos/${repo}/issues/${pr_number}"

issue_json="$(
  curl -fsSL \
    -H "Authorization: Bearer ${token}" \
    -H "Accept: application/vnd.github+json" \
    -H "X-GitHub-Api-Version: 2022-11-28" \
    "${issue_url}"
)" || fail "failed to fetch PR labels via GitHub API (repo=${repo}, pr=${pr_number})"

mapfile -t labels < <(
  python3 - <<'PY' <<<"${issue_json}"
import json, sys
data = json.load(sys.stdin)
for l in data.get("labels", []) or []:
  name = l.get("name")
  if name:
    print(name)
PY
)

intent_labels=()
unknown_type_labels=()

for label in "${labels[@]:-}"; do
  if [[ "${label}" == type:* ]]; then
    is_allowed="false"
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

allowed_csv="$(IFS='|'; echo "${allowed_intent_labels[*]}")"

if (( ${#unknown_type_labels[@]} > 0 )); then
  fail "unknown intent label(s): $(IFS=','; echo "${unknown_type_labels[*]}") (allowed: ${allowed_csv})"
fi

if (( ${#intent_labels[@]} == 0 )); then
  fail "missing intent label: must choose exactly one of: ${allowed_csv}"
fi

if (( ${#intent_labels[@]} > 1 )); then
  fail "intent labels are mutually exclusive; found: $(IFS=','; echo "${intent_labels[*]}")"
fi

intent_label="${intent_labels[0]}"
should_release="false"
bump_level="none"

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
    fail "unexpected intent label: ${intent_label}"
    ;;
esac

echo "label-gate: pr=${pr_number} intent_label=${intent_label} should_release=${should_release} bump_level=${bump_level}"

write_output "intent_label" "${intent_label}"
write_output "should_release" "${should_release}"
write_output "bump_level" "${bump_level}"
