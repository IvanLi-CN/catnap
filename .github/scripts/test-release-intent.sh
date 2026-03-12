#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TARGET_SCRIPT="${SCRIPT_DIR}/release-intent.sh"
FIXTURES_DIR="${SCRIPT_DIR}/fixtures/release-intent"
TMP_DIR="$(mktemp -d)"
BIN_DIR="${TMP_DIR}/bin"
mkdir -p "${BIN_DIR}"
trap 'rm -rf "${TMP_DIR}"' EXIT

cat > "${BIN_DIR}/curl" <<'CURL'
#!/usr/bin/env bash
set -euo pipefail
url="${@: -1}"

case "${url}" in
  */commits/*/pulls)
    status="${TEST_PULLS_STATUS:-0}"
    payload_path="${TEST_PULLS_JSON_PATH:-}"
    payload="${TEST_PULLS_JSON:-[]}"
    ;;
  */issues/*)
    status="${TEST_ISSUE_STATUS:-0}"
    payload_path="${TEST_ISSUE_JSON_PATH:-}"
    payload="${TEST_ISSUE_JSON:-{}}"
    ;;
  *)
    echo "curl stub: unexpected url: ${url}" >&2
    exit 64
    ;;
esac

if [[ "${status}" != "0" ]]; then
  exit "${status}"
fi

if [[ -n "${payload_path}" ]]; then
  cat "${payload_path}"
else
  printf '%s' "${payload}"
fi
CURL
chmod +x "${BIN_DIR}/curl"

cat > "${BIN_DIR}/git" <<'GIT'
#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "log" && "${2:-}" == "-1" && "${3:-}" == "--format=%s" ]]; then
  printf '%s\n' "${TEST_GIT_SUBJECT:-}"
  exit 0
fi

echo "git stub: unsupported args: $*" >&2
exit 64
GIT
chmod +x "${BIN_DIR}/git"

fail() {
  echo "test-release-intent: $*" >&2
  exit 1
}

read_output() {
  local file="$1"
  local key="$2"
  python3 - "$file" "$key" <<'PY'
import pathlib, sys
path = pathlib.Path(sys.argv[1])
key = sys.argv[2]
for line in path.read_text().splitlines():
    if not line.startswith(f"{key}="):
        continue
    print(line.split("=", 1)[1])
    break
PY
}

run_case() {
  local name="$1"
  local event_name="$2"
  local ref="$3"
  local ref_name="$4"
  local pulls_fixture="$5"
  local issue_fixture="$6"
  local subject="$7"
  local expected_should_release="$8"
  local expected_bump_level="$9"
  local expected_intent_label="${10}"
  local expected_pr_number="${11}"

  local output_file="${TMP_DIR}/${name}.out"
  local log_file="${TMP_DIR}/${name}.log"

  PATH="${BIN_DIR}:${PATH}" \
  GITHUB_TOKEN="test-token" \
  GITHUB_REPOSITORY="IvanLi-CN/catnap" \
  GITHUB_SHA="cafc2179b10fa846d9ac0302d1c129618be7e13b" \
  GITHUB_EVENT_NAME="${event_name}" \
  GITHUB_REF="${ref}" \
  GITHUB_REF_NAME="${ref_name}" \
  GITHUB_API_URL="https://example.invalid" \
  GITHUB_OUTPUT="${output_file}" \
  BUMP_LEVEL="minor" \
  TEST_PULLS_JSON_PATH="${FIXTURES_DIR}/${pulls_fixture}" \
  TEST_ISSUE_JSON_PATH="${FIXTURES_DIR}/${issue_fixture}" \
  TEST_GIT_SUBJECT="${subject}" \
  bash "${TARGET_SCRIPT}" >"${log_file}" 2>&1

  local actual_should_release
  local actual_bump_level
  local actual_intent_label
  local actual_pr_number
  actual_should_release="$(read_output "${output_file}" should_release)"
  actual_bump_level="$(read_output "${output_file}" bump_level)"
  actual_intent_label="$(read_output "${output_file}" intent_label)"
  actual_pr_number="$(read_output "${output_file}" pr_number)"

  [[ "${actual_should_release}" == "${expected_should_release}" ]] || fail "${name}: should_release=${actual_should_release} (expected ${expected_should_release})"
  [[ "${actual_bump_level}" == "${expected_bump_level}" ]] || fail "${name}: bump_level=${actual_bump_level} (expected ${expected_bump_level})"
  [[ "${actual_intent_label}" == "${expected_intent_label}" ]] || fail "${name}: intent_label=${actual_intent_label} (expected ${expected_intent_label})"
  [[ "${actual_pr_number}" == "${expected_pr_number}" ]] || fail "${name}: pr_number=${actual_pr_number} (expected ${expected_pr_number})"
}

run_case \
  api_hit \
  push \
  refs/heads/main \
  main \
  pulls-pr-60.json \
  issue-pr-60-minor.json \
  "feat: direct API mapping" \
  true \
  minor \
  type:minor \
  60

run_case \
  merge_subject_fallback \
  push \
  refs/heads/main \
  main \
  pulls-empty.json \
  issue-pr-60-minor.json \
  "Merge pull request #60 from IvanLi-CN/th/low-pressure-discovery-refresh" \
  true \
  minor \
  type:minor \
  60

run_case \
  squash_subject_fallback \
  push \
  refs/heads/main \
  main \
  pulls-empty.json \
  issue-pr-60-minor.json \
  "feat: reduce upstream pressure during catalog discovery (#60)" \
  true \
  minor \
  type:minor \
  60

run_case \
  no_fallback_skip \
  push \
  refs/heads/main \
  main \
  pulls-empty.json \
  issue-pr-60-minor.json \
  "feat: direct push without pr mapping" \
  false \
  none \
  none \
  none

run_case \
  invalid_label_skip \
  push \
  refs/heads/main \
  main \
  pulls-empty.json \
  issue-pr-60-invalid.json \
  "fix: release intent fallback should stay conservative (#60)" \
  false \
  none \
  invalid \
  60

run_case \
  manual_publish_main \
  workflow_dispatch \
  refs/heads/main \
  main \
  pulls-empty.json \
  issue-pr-60-minor.json \
  "ignored" \
  true \
  minor \
  manual:minor \
  none

run_case \
  manual_branch_dispatch_skip \
  workflow_dispatch \
  refs/heads/th/test-branch \
  th/test-branch \
  pulls-empty.json \
  issue-pr-60-minor.json \
  "ignored" \
  false \
  none \
  none \
  none

echo "test-release-intent: all cases passed"
