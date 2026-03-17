#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
import json
import re
import sys
from pathlib import Path
from typing import Any


class ContractError(RuntimeError):
    pass


REQUIRED_CHECKS = {
    "PR Label Gate",
    "Path Gate",
    "Lint & Checks",
    "Backend Tests",
    "Release Chain Smoke (PR)",
    "Review Policy Gate",
}


def load_module(path: Path):
    spec = importlib.util.spec_from_file_location("metadata_gate", path)
    if spec is None or spec.loader is None:
        raise ContractError(f"Unable to load module from {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ContractError(message)


def require_text(text: str, needle: str, where: str) -> None:
    require(needle in text, f"{where}: missing required text {needle!r}")


def forbid_text(text: str, needle: str, where: str) -> None:
    require(needle not in text, f"{where}: unexpected text {needle!r}")


def validate_quality_gates(path: Path) -> None:
    payload = json.loads(path.read_text())
    branch_policy = payload["policy"]["branch_protection"]
    require(branch_policy.get("require_merge_queue") is False, "quality-gates.json: require_merge_queue must be false")
    required_checks = set(payload.get("required_checks", []))
    require(required_checks == REQUIRED_CHECKS, f"quality-gates.json: required_checks drifted: {sorted(required_checks)}")

    expected = {
        (entry.get("workflow"), tuple(entry.get("jobs", [])))
        for entry in payload.get("expected_pr_workflows", [])
        if isinstance(entry, dict)
    }
    require(
        ("PR Label Gate", ("PR Label Gate",)) in expected,
        "quality-gates.json: expected_pr_workflows must include PR Label Gate workflow/job",
    )
    require(
        ("Review Policy", ("Review Policy Gate",)) in expected,
        "quality-gates.json: expected_pr_workflows must include Review Policy workflow/job",
    )
    require(
        ("CI PR", ("Path Gate", "Lint & Checks", "Backend Tests", "Release Chain Smoke (PR)")) in expected,
        "quality-gates.json: expected_pr_workflows must include CI PR required jobs",
    )


def validate_ci_pr(path: Path) -> None:
    text = path.read_text()
    require_text(text, "name: CI PR", "ci-pr.yml")
    require_text(text, "merge_group:", "ci-pr.yml")
    require_text(text, "checks_requested", "ci-pr.yml")
    require_text(text, "Release Chain Smoke (PR)", "ci-pr.yml")
    require_text(text, "test-release-snapshot.sh", "ci-pr.yml")
    forbid_text(text, "release-intent.sh", "ci-pr.yml")
    forbid_text(text, "workflow_dispatch:", "ci-pr.yml")
    forbid_text(text, "pull_request_target:", "ci-pr.yml")


def validate_ci_main(path: Path) -> None:
    text = path.read_text()
    require_text(text, "name: CI Main", "ci-main.yml")
    require_text(text, "push:", "ci-main.yml")
    require_text(text, "branches: [main]", "ci-main.yml")
    require_text(text, "Release Snapshot", "ci-main.yml")
    require_text(text, "release_snapshot.py ensure", "ci-main.yml")
    forbid_text(text, "workflow_dispatch:", "ci-main.yml")
    forbid_text(text, "release-intent.sh", "ci-main.yml")


def validate_release(path: Path) -> None:
    text = path.read_text()
    require_text(text, "name: Release", "release.yml")
    require_text(text, "workflow_run:", "release.yml")
    require_text(text, "CI Main", "release.yml")
    require_text(text, "commit_sha", "release.yml")
    require_text(text, "release_snapshot.py next-pending", "release.yml")
    require_text(text, "release_snapshot.py export", "release.yml")
    require_text(text, "release_snapshot.py ensure", "release.yml")
    require_text(text, "RELEASE_TOOLING_ROOT: ${{ github.workspace }}/.release-tooling", "release.yml")
    require_text(text, "CATNAP_DOCKER_BINARY_SOURCE_ROOT: ${{ github.workspace }}/target", "release.yml")
    require_text(text, "CATNAP_DOCKER_BINARY_OUTPUT_ROOT: ${{ github.workspace }}/dist/docker", "release.yml")
    require_text(text, "Resolve GitHub release auth token", "release.yml")
    require_text(text, "RELEASE_WORKFLOW_TOKEN", "release.yml")
    require_text(text, "Require workflow-capable token for workflow commits", "release.yml")
    require_text(text, "Probe workflow-commit tag permission", "release.yml")
    require_text(text, "Configured RELEASE_WORKFLOW_TOKEN still cannot tag workflow-changing commit", "release.yml")
    require_text(text, "persist-credentials: false", "release.yml")
    require_text(text, "publish_github_release.py", "release.yml")
    require_text(text, "Ensure release tag exists on target commit", "release.yml")
    require_text(text, "git config --local --unset-all http.https://github.com/.extraheader || true", "release.yml")
    require_text(text, "git tag \"${RELEASE_TAG}\" \"${TARGET_SHA}\"", "release.yml")
    require_text(text, "--artifacts-dir dist/release-assets", "release.yml")
    require_text(text, "Verify release tag points to target commit", "release.yml")
    require_text(text, "issues: write", "release.yml")
    require_text(text, "pull-requests: write", "release.yml")
    require_text(text, "github-token: ${{ steps.release-auth.outputs.token }}", "release.yml")
    require_text(text, "codex-release-version-comment", "release.yml")
    forbid_text(text, "bump_level", "release.yml")
    forbid_text(text, "release-intent.sh", "release.yml")
    forbid_text(text, "git push origin \"${tag}\"", "release.yml")
    forbid_text(text, "ncipollo/release-action@v1", "release.yml")


def validate_label_gate(path: Path) -> None:
    text = path.read_text()
    require(re.search(r"(?m)^\s*pull_request:\s*$", text) is not None, "label-gate.yml: must trigger on pull_request")
    require(re.search(r"(?m)^\s*merge_group:\s*$", text) is not None, "label-gate.yml: must trigger on merge_group")
    require(re.search(r"(?m)^\s*pull_request_target:\s*$", text) is None, "label-gate.yml: must not trigger on pull_request_target")
    require_text(text, "channel:stable", "label-gate.yml")
    require_text(text, "channel:rc", "label-gate.yml")
    require_text(text, "PR must have exactly one channel:* label", "label-gate.yml")
    require_text(text, "GET /repos/{owner}/{repo}/commits/{commit_sha}/pulls", "label-gate.yml")
    forbid_text(text, "actions/checkout", "label-gate.yml")
    forbid_text(text, "metadata_gate.py", "label-gate.yml")


def validate_review_policy(path: Path) -> None:
    text = path.read_text()
    require(re.search(r"(?m)^\s*pull_request:\s*$", text) is not None, "review-policy.yml: must trigger on pull_request")
    require(re.search(r"(?m)^\s*pull_request_review:\s*$", text) is not None, "review-policy.yml: must trigger on pull_request_review")
    require(re.search(r"(?m)^\s*merge_group:\s*$", text) is not None, "review-policy.yml: must trigger on merge_group")
    require(re.search(r"(?m)^\s*pull_request_target:\s*$", text) is None, "review-policy.yml: must not trigger on pull_request_target")
    require_text(text, "GET /repos/{owner}/{repo}/commits/{commit_sha}/pulls", "review-policy.yml")
    require_text(text, "GET /repos/{owner}/{repo}/collaborators/{username}/permission", "review-policy.yml")
    require_text(text, "GET /repos/{owner}/{repo}/pulls/{pull_number}/reviews", "review-policy.yml")
    forbid_text(text, "actions/checkout", "review-policy.yml")
    forbid_text(text, "metadata_gate.py", "review-policy.yml")


def validate_merge_group_helpers(module: Any, fixtures_dir: Path) -> None:
    associated_payload = json.loads((fixtures_dir / "merge-group-associated-open.json").read_text())
    resolved = module.resolve_merge_group_pull_numbers_from_data(
        "gh-readonly-queue/main/pr-42-a1b2c3d4/pr-57-ffeeddcc",
        "refs/heads/main",
        associated_payload,
    )
    require(resolved == [42, 57], f"metadata_gate: unexpected associated merge queue set {resolved}")

    anchors = module.parse_pull_numbers_from_text("gh-readonly-queue/main/pr-42-a1b2c3d4/pr-57-ffeeddcc")
    require(anchors == [42, 57], f"metadata_gate: anchor parsing drifted: {anchors}")

    documented_single_anchor = module.resolve_merge_group_pull_numbers_from_data(
        "gh-readonly-queue/main/pr-57-ffeeddcc",
        "refs/heads/main",
        associated_payload,
    )
    require(
        documented_single_anchor == [42, 57],
        f"metadata_gate: single-anchor merge queue set drifted: {documented_single_anchor}",
    )

    try:
        module.resolve_merge_group_pull_numbers_from_data(
            "gh-readonly-queue/main/pr-999-deadbeef",
            "refs/heads/main",
            associated_payload,
        )
    except module.GateError as exc:
        require("mismatch" in str(exc), f"metadata_gate: unexpected mismatch error {exc}")
    else:
        raise ContractError("metadata_gate: missing merge queue mismatch failure")

    try:
        module.resolve_merge_group_pull_numbers_from_data(
            "refs/heads/main",
            "refs/heads/main",
            associated_payload,
        )
    except module.GateError as exc:
        require("could not be proven" in str(exc), f"metadata_gate: unexpected proof error {exc}")
    else:
        raise ContractError("metadata_gate: missing merge queue proof failure")


def main() -> int:
    repo_root = Path(__file__).resolve().parents[2]
    scripts_dir = repo_root / ".github" / "scripts"
    fixtures_dir = scripts_dir / "fixtures" / "quality-gates"

    try:
      module = load_module(scripts_dir / "metadata_gate.py")
      validate_quality_gates(repo_root / ".github" / "quality-gates.json")
      validate_ci_pr(repo_root / ".github" / "workflows" / "ci-pr.yml")
      validate_ci_main(repo_root / ".github" / "workflows" / "ci-main.yml")
      validate_release(repo_root / ".github" / "workflows" / "release.yml")
      validate_label_gate(repo_root / ".github" / "workflows" / "label-gate.yml")
      validate_review_policy(repo_root / ".github" / "workflows" / "review-policy.yml")
      validate_merge_group_helpers(module, fixtures_dir)
      require(not (repo_root / ".github" / "workflows" / "ci.yml").exists(), "ci.yml must be retired")
      require(not (repo_root / ".github" / "workflows" / "release-backfill.yml").exists(), "release-backfill.yml must be retired")
    except ContractError as exc:
      print(f"[quality-gates-contract] {exc}", file=sys.stderr)
      return 1

    print("[quality-gates-contract] metadata workflow contract checks passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
