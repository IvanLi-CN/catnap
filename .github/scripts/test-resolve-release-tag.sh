#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
python3 - <<'PY' "$repo_root/.github/scripts/resolve_release_tag.py"
from __future__ import annotations

import subprocess
import sys
import tempfile
from pathlib import Path


script_path = Path(sys.argv[1])


def run(*args: str, cwd: Path, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args],
        cwd=cwd,
        check=check,
        text=True,
        capture_output=True,
    )


def python_run(*args: str, cwd: Path, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["python3", str(script_path), *args],
        cwd=cwd,
        check=check,
        text=True,
        capture_output=True,
    )


def parse_output(stdout: str) -> dict[str, str]:
    outputs: dict[str, str] = {}
    for line in stdout.splitlines():
        if "=" not in line:
            continue
        key, value = line.split("=", 1)
        outputs[key] = value
    return outputs


with tempfile.TemporaryDirectory(prefix="resolve-release-tag-") as tmp:
    repo = Path(tmp)
    run("init", cwd=repo)
    run("config", "user.name", "Test User", cwd=repo)
    run("config", "user.email", "test@example.com", cwd=repo)
    run("checkout", "-b", "main", cwd=repo)

    (repo / "README.md").write_text("base\n")
    run("add", "README.md", cwd=repo)
    run("commit", "-m", "base", cwd=repo)

    (repo / ".github" / "workflows").mkdir(parents=True)
    (repo / ".github" / "workflows" / "release.yml").write_text("name: release\n")
    run("add", ".github/workflows/release.yml", cwd=repo)
    run("commit", "-m", "workflow target", cwd=repo)
    workflow_target_sha = run("rev-parse", "HEAD", cwd=repo).stdout.strip()

    (repo / "README.md").write_text("carrier\n")
    run("add", "README.md", cwd=repo)
    run("commit", "-m", "carrier", cwd=repo)
    carrier_sha = run("rev-parse", "HEAD", cwd=repo).stdout.strip()

    (repo / "README.md").write_text("plain target\n")
    run("add", "README.md", cwd=repo)
    run("commit", "-m", "plain target", cwd=repo)
    plain_target_sha = run("rev-parse", "HEAD", cwd=repo).stdout.strip()

    # Existing historical tag should be reused even when carrier mode is active.
    run("tag", "v1.2.3", workflow_target_sha, cwd=repo)
    result = python_run(
        "--release-tag",
        "v1.2.3",
        "--target-sha",
        workflow_target_sha,
        "--candidate-release-carrier-sha",
        carrier_sha,
        cwd=repo,
    )
    outputs = parse_output(result.stdout)
    assert outputs["release_create_mode"] == "existing_tag"
    assert outputs["resolved_release_tag_sha"] == workflow_target_sha
    assert "reusing the historical tag instead of carrier" in result.stdout

    # Existing carrier-backed tag should still be accepted.
    run("tag", "v1.2.4", carrier_sha, cwd=repo)
    result = python_run(
        "--release-tag",
        "v1.2.4",
        "--target-sha",
        workflow_target_sha,
        "--candidate-release-carrier-sha",
        carrier_sha,
        cwd=repo,
    )
    outputs = parse_output(result.stdout)
    assert outputs["release_create_mode"] == "existing_tag"
    assert outputs["resolved_release_tag_sha"] == carrier_sha

    # Missing tag in carrier mode should use API default-branch creation.
    result = python_run(
        "--release-tag",
        "v1.2.5",
        "--target-sha",
        workflow_target_sha,
        "--candidate-release-carrier-sha",
        carrier_sha,
        cwd=repo,
    )
    outputs = parse_output(result.stdout)
    assert outputs["release_create_mode"] == "api_default_branch"
    assert outputs["resolved_release_tag_sha"] == carrier_sha

    # Missing tag on a non-workflow target can be created directly by the API.
    result = python_run(
        "--release-tag",
        "v1.2.6",
        "--target-sha",
        plain_target_sha,
        "--candidate-release-target-sha",
        plain_target_sha,
        "--candidate-release-carrier-sha",
        plain_target_sha,
        cwd=repo,
    )
    outputs = parse_output(result.stdout)
    assert outputs["release_create_mode"] == "api_target_sha"
    assert outputs["resolved_release_tag_sha"] == plain_target_sha

    # Missing tag on a workflow-changing target still requires precreation.
    result = python_run(
        "--release-tag",
        "v1.2.7",
        "--target-sha",
        workflow_target_sha,
        "--candidate-release-target-sha",
        workflow_target_sha,
        "--candidate-release-carrier-sha",
        workflow_target_sha,
        cwd=repo,
    )
    outputs = parse_output(result.stdout)
    assert outputs["release_create_mode"] == "precreated_tag"
    assert outputs["resolved_release_tag_sha"] == workflow_target_sha

    # Existing unexpected tag target must still fail.
    run("tag", "v1.2.8", plain_target_sha, cwd=repo)
    result = python_run(
        "--release-tag",
        "v1.2.8",
        "--target-sha",
        workflow_target_sha,
        "--candidate-release-carrier-sha",
        carrier_sha,
        cwd=repo,
        check=False,
    )
    assert result.returncode == 1
    assert "expected one of" in result.stderr

print("test-resolve-release-tag: all checks passed")
PY
