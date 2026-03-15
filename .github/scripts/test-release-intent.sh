#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TARGET_SCRIPT="${SCRIPT_DIR}/release-intent.sh"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

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

create_repo_with_commit() {
  local repo_dir="$1"
  local subject="$2"

  mkdir -p "${repo_dir}"
  git -C "${repo_dir}" init -q
  git -C "${repo_dir}" config user.name "Catnap Tests"
  git -C "${repo_dir}" config user.email "tests@example.com"
  cat > "${repo_dir}/Cargo.toml" <<'EOF'
[package]
name = "catnap"
version = "0.1.0"
edition = "2021"
EOF
  echo "seed" > "${repo_dir}/README.md"
  git -C "${repo_dir}" add Cargo.toml README.md
  git -C "${repo_dir}" commit -q -m "${subject}"
  git -C "${repo_dir}" rev-parse HEAD
}

write_fixture() {
  local fixture_path="$1"
  local commit_sha="$2"
  local mode="$3"
  local label_mode="$4"

  python3 - "$fixture_path" "$commit_sha" "$mode" "$label_mode" <<'PY'
import json
import pathlib
import sys

fixture_path = pathlib.Path(sys.argv[1])
commit_sha = sys.argv[2]
mode = sys.argv[3]
label_mode = sys.argv[4]

issues = {
    "minor": ["type:minor", "channel:stable"],
    "invalid": ["type:weird", "channel:stable"],
    "legacy-minor": ["type:minor"],
    "rc-patch": ["type:patch", "channel:rc"],
}[label_mode]

payload = {
    "commits_pulls": {},
    "closed_pulls": [],
    "pulls": {},
    "issues": {
        "60": issues,
    },
}

if mode == "api":
    payload["commits_pulls"][commit_sha] = [{"number": 60, "merged_at": "2026-03-15T00:00:00Z", "base": {"ref": "main"}}]
elif mode == "merge-fallback":
    payload["closed_pulls"].append(
        {
            "number": 60,
            "merge_commit_sha": commit_sha,
            "merged_at": "2026-03-15T00:00:00Z",
            "base": {"ref": "main"},
        }
    )
elif mode == "subject":
    payload["pulls"]["60"] = {
        "number": 60,
        "merged_at": "2026-03-15T00:00:00Z",
        "base": {"ref": "main"},
    }
else:
    raise SystemExit(f"unexpected mode: {mode}")

fixture_path.write_text(json.dumps(payload))
PY
}

run_case() {
  local name="$1"
  local event_name="$2"
  local ref="$3"
  local ref_name="$4"
  local subject="$5"
  local fixture_mode="$6"
  local label_mode="$7"
  local expected_should_release="$8"
  local expected_bump_level="$9"
  local expected_intent_label="${10}"
  local expected_pr_number="${11}"
  local expected_channel_label="${12}"
  local expected_channel_source="${13}"

  local repo_dir="${TMP_DIR}/${name}-repo"
  local sha
  sha="$(create_repo_with_commit "${repo_dir}" "${subject}")"

  local fixture_path="${TMP_DIR}/${name}.json"
  if [[ "${fixture_mode}" != "none" ]]; then
    write_fixture "${fixture_path}" "${sha}" "${fixture_mode}" "${label_mode}"
  else
    printf '{}\n' > "${fixture_path}"
  fi

  local output_file="${TMP_DIR}/${name}.out"
  local log_file="${TMP_DIR}/${name}.log"

  GITHUB_TOKEN="test-token" \
  GITHUB_REPOSITORY="IvanLi-CN/catnap" \
  GITHUB_SHA="${sha}" \
  GITHUB_EVENT_NAME="${event_name}" \
  GITHUB_REF="${ref}" \
  GITHUB_REF_NAME="${ref_name}" \
  GITHUB_WORKSPACE="${repo_dir}" \
  GITHUB_OUTPUT="${output_file}" \
  BUMP_LEVEL="minor" \
  TEST_RELEASE_PLAN_FIXTURES="${fixture_path}" \
  bash "${TARGET_SCRIPT}" >"${log_file}" 2>&1

  local actual_should_release
  local actual_bump_level
  local actual_intent_label
  local actual_pr_number
  local actual_channel_label
  local actual_channel_source
  actual_should_release="$(read_output "${output_file}" should_release)"
  actual_bump_level="$(read_output "${output_file}" bump_level)"
  actual_intent_label="$(read_output "${output_file}" intent_label)"
  actual_pr_number="$(read_output "${output_file}" pr_number)"
  actual_channel_label="$(read_output "${output_file}" channel_label)"
  actual_channel_source="$(read_output "${output_file}" channel_source)"

  [[ "${actual_should_release}" == "${expected_should_release}" ]] || fail "${name}: should_release=${actual_should_release} (expected ${expected_should_release})"
  [[ "${actual_bump_level}" == "${expected_bump_level}" ]] || fail "${name}: bump_level=${actual_bump_level} (expected ${expected_bump_level})"
  [[ "${actual_intent_label}" == "${expected_intent_label}" ]] || fail "${name}: intent_label=${actual_intent_label} (expected ${expected_intent_label})"
  [[ "${actual_pr_number}" == "${expected_pr_number}" ]] || fail "${name}: pr_number=${actual_pr_number} (expected ${expected_pr_number})"
  [[ "${actual_channel_label}" == "${expected_channel_label}" ]] || fail "${name}: channel_label=${actual_channel_label} (expected ${expected_channel_label})"
  [[ "${actual_channel_source}" == "${expected_channel_source}" ]] || fail "${name}: channel_source=${actual_channel_source} (expected ${expected_channel_source})"
}

run_case \
  api_hit \
  push \
  refs/heads/main \
  main \
  "feat: direct API mapping" \
  api \
  minor \
  true \
  minor \
  type:minor \
  60 \
  channel:stable \
  label

run_case \
  merge_commit_fallback \
  push \
  refs/heads/main \
  main \
  "feat(products): add manual region refresh" \
  merge-fallback \
  legacy-minor \
  true \
  minor \
  type:minor \
  60 \
  channel:stable \
  legacy-default

run_case \
  squash_subject_fallback \
  push \
  refs/heads/main \
  main \
  "feat: reduce upstream pressure during catalog discovery (#60)" \
  subject \
  minor \
  true \
  minor \
  type:minor \
  60 \
  channel:stable \
  label

run_case \
  no_fallback_skip \
  push \
  refs/heads/main \
  main \
  "feat: direct push without pr mapping" \
  subject \
  minor \
  false \
  none \
  none \
  none \
  none \
  none

run_case \
  invalid_label_skip \
  push \
  refs/heads/main \
  main \
  "fix: release intent fallback should stay conservative (#60)" \
  subject \
  invalid \
  false \
  none \
  none \
  none \
  none \
  none

run_case \
  manual_publish_main \
  workflow_dispatch \
  refs/heads/main \
  main \
  "ignored" \
  none \
  minor \
  true \
  minor \
  manual:minor \
  none \
  channel:stable \
  manual

run_case \
  manual_branch_dispatch_skip \
  workflow_dispatch \
  refs/heads/th/test-branch \
  th/test-branch \
  "ignored" \
  none \
  minor \
  false \
  none \
  none \
  none \
  none \
  none

echo "test-release-intent: all cases passed"
