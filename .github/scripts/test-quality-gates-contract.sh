#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
python3 "$repo_root/.github/scripts/check_quality_gates_contract.py"

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

metadata_regression_repo="$tmp_dir/metadata-regression-repo"
mkdir -p "$metadata_regression_repo"
tar --exclude='.git' -C "$repo_root" -cf - . | tar -C "$metadata_regression_repo" -xf -
python3 - <<'PY' "$metadata_regression_repo"
from pathlib import Path
import sys

repo = Path(sys.argv[1])
path = repo / ".github/workflows/ci-pr.yml"
text = path.read_text()
needle = "github.event_name == 'pull_request' && github.event.action == 'edited'"
if needle not in text:
    raise SystemExit("missing metadata event selector")
path.write_text(text.replace(needle, "github.event.action == 'edited'", 1))
PY

if python3 "$metadata_regression_repo/.github/scripts/check_quality_gates_contract.py" >/dev/null 2>"$tmp_dir/metadata-regression.log"; then
  echo "expected metadata concurrency selector regression to fail" >&2
  exit 1
fi

grep -q "ci-pr.yml: missing required text" "$tmp_dir/metadata-regression.log"

bash "$repo_root/.github/scripts/test-inline-metadata-workflows.sh"
bash "$repo_root/.github/scripts/test-resolve-release-tag.sh"

echo "test-quality-gates-contract: all checks passed"
