#!/usr/bin/env python3
from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path


WORKFLOW_DIR_PREFIX = ".github/workflows/"


class ReleaseTagResolutionError(RuntimeError):
    pass


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Resolve how a release tag should be created or reused for a release publish run."
    )
    parser.add_argument("--release-tag", required=True)
    parser.add_argument("--target-sha", required=True)
    parser.add_argument("--candidate-release-target-sha", default="")
    parser.add_argument("--candidate-release-carrier-sha", default="")
    parser.add_argument("--github-output", default="")
    return parser.parse_args()


def git(*args: str) -> str:
    result = subprocess.run(["git", *args], check=True, text=True, capture_output=True)
    return result.stdout.strip()


def tag_commit_sha(tag: str) -> str | None:
    result = subprocess.run(
        ["git", "rev-parse", "-q", "--verify", f"refs/tags/{tag}"],
        check=False,
        text=True,
        capture_output=True,
    )
    if result.returncode != 0:
        return None
    return git("rev-list", "-n", "1", tag)


def commit_changes_workflows(target_sha: str) -> bool:
    changed = git("diff-tree", "--no-commit-id", "--name-only", "-r", target_sha)
    return any(path.startswith(WORKFLOW_DIR_PREFIX) for path in changed.splitlines())


def emit_outputs(outputs: dict[str, str], github_output: str) -> None:
    for key, value in outputs.items():
        print(f"{key}={value}")
    if not github_output:
        return
    path = Path(github_output)
    with path.open("a", encoding="utf-8") as handle:
        for key, value in outputs.items():
            handle.write(f"{key}={value}\n")


def main() -> int:
    args = parse_args()
    expected_sha = args.candidate_release_target_sha or args.candidate_release_carrier_sha
    if not expected_sha:
        print(f"Missing release carrier SHA for {args.release_tag}", file=sys.stderr)
        return 1

    actual_tag_sha = tag_commit_sha(args.release_tag)
    if actual_tag_sha is not None:
        accepted_shas = {expected_sha, args.target_sha}
        if actual_tag_sha not in accepted_shas:
            accepted = ", ".join(sorted(accepted_shas))
            print(
                f"Release tag {args.release_tag} already points to {actual_tag_sha} "
                f"(expected one of {accepted})",
                file=sys.stderr,
            )
            return 1

        if actual_tag_sha == args.target_sha and actual_tag_sha != expected_sha:
            print(
                f"Release tag {args.release_tag} already exists on snapshot target {actual_tag_sha}; "
                f"reusing the historical tag instead of carrier {expected_sha}."
            )
        else:
            print(f"Release tag {args.release_tag} already exists on {actual_tag_sha}.")

        emit_outputs(
            {
                "release_create_mode": "existing_tag",
                "resolved_release_tag_sha": actual_tag_sha,
            },
            args.github_output,
        )
        return 0

    if not args.candidate_release_target_sha:
        print(
            f"GitHub Release API will create {args.release_tag} from default branch head "
            f"{args.candidate_release_carrier_sha}."
        )
        emit_outputs(
            {
                "release_create_mode": "api_default_branch",
                "resolved_release_tag_sha": args.candidate_release_carrier_sha,
            },
            args.github_output,
        )
        return 0

    if not commit_changes_workflows(args.target_sha):
        print(f"Release target does not change workflows; GitHub Release API will create tag {args.release_tag}.")
        emit_outputs(
            {
                "release_create_mode": "api_target_sha",
                "resolved_release_tag_sha": args.candidate_release_target_sha,
            },
            args.github_output,
        )
        return 0

    emit_outputs(
        {
            "release_create_mode": "precreated_tag",
            "resolved_release_tag_sha": args.candidate_release_target_sha,
        },
        args.github_output,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
