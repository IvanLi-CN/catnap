#!/usr/bin/env python3
from __future__ import annotations

import json
import subprocess
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
SCRIPT = REPO_ROOT / ".github" / "scripts" / "release_plan.py"


def run(cmd: list[str], cwd: Path) -> str:
    result = subprocess.run(cmd, cwd=cwd, check=True, text=True, capture_output=True)
    return result.stdout.strip()


def git(repo: Path, *args: str) -> str:
    return run(["git", *args], repo)


def init_repo(tmpdir: Path) -> tuple[Path, str]:
    repo = tmpdir / "repo"
    repo.mkdir()
    git(repo, "init", "-q")
    git(repo, "config", "user.name", "Catnap Tests")
    git(repo, "config", "user.email", "tests@example.com")
    (repo / "Cargo.toml").write_text(
        "[package]\nname = \"catnap\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"
    )
    (repo / "README.md").write_text("base\n")
    git(repo, "add", "Cargo.toml", "README.md")
    git(repo, "commit", "-q", "-m", "chore: bootstrap release plan fixtures")
    base_sha = git(repo, "rev-parse", "HEAD")
    git(repo, "tag", "v0.8.1")
    return repo, base_sha


def commit(repo: Path, filename: str, content: str, subject: str) -> str:
    (repo / filename).write_text(content)
    git(repo, "add", filename)
    git(repo, "commit", "-q", "-m", subject)
    return git(repo, "rev-parse", "HEAD")


def plan(repo: Path, fixture_path: Path, target_sha: str) -> dict:
    result = subprocess.run(
        [
            "python3",
            str(SCRIPT),
            "plan",
            "--repo",
            "IvanLi-CN/catnap",
            "--repo-root",
            str(repo),
            "--fixtures",
            str(fixture_path),
            "--target-sha",
            target_sha,
            "--target-ref",
            "main",
        ],
        check=True,
        text=True,
        capture_output=True,
        cwd=REPO_ROOT,
    )
    return json.loads(result.stdout)


def inspect(repo: Path, fixture_path: Path, target_sha: str) -> dict:
    result = subprocess.run(
        [
            "python3",
            str(SCRIPT),
            "inspect-commit",
            "--repo",
            "IvanLi-CN/catnap",
            "--repo-root",
            str(repo),
            "--fixtures",
            str(fixture_path),
            "--target-sha",
            target_sha,
        ],
        check=True,
        text=True,
        capture_output=True,
        cwd=REPO_ROOT,
    )
    return json.loads(result.stdout)


def assert_equal(actual, expected, message: str) -> None:
    if actual != expected:
        raise AssertionError(f"{message}: expected {expected!r}, got {actual!r}")


def test_sequential_stable_reconciliation() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        repo, _ = init_repo(Path(tmp))
        sha70 = commit(repo, "feature-70.txt", "70\n", "feat(products): add manual region refresh")
        sha69 = commit(repo, "feature-69.txt", "69\n", "feat: support telegram multi-target delivery")
        fixture_path = Path(tmp) / "fixtures.json"
        fixture_path.write_text(
            json.dumps(
                {
                    "commits_pulls": {},
                    "closed_pulls": [
                        {
                            "number": 70,
                            "merge_commit_sha": sha70,
                            "merged_at": "2026-03-15T03:43:00Z",
                            "base": {"ref": "main"},
                        },
                        {
                            "number": 69,
                            "merge_commit_sha": sha69,
                            "merged_at": "2026-03-15T04:57:00Z",
                            "base": {"ref": "main"},
                        },
                    ],
                    "issues": {
                        "70": ["type:minor"],
                        "69": ["type:minor", "channel:stable"],
                    },
                }
            )
        )
        payload = plan(repo, fixture_path, sha69)
        assert_equal(payload["base_stable_tag"], "v0.8.1", "base stable tag")
        assert_equal(payload["release_count"], 2, "stable release count")
        entries = payload["entries"]
        assert_equal([entry["version"] for entry in entries], ["0.9.0", "0.10.0"], "planned stable versions")
        assert_equal([entry["tag"] for entry in entries], ["v0.9.0", "v0.10.0"], "planned stable tags")
        assert_equal([entry["channel_source"] for entry in entries], ["legacy-default", "label"], "channel sources")
        assert_equal([entry["publish_latest"] for entry in entries], [False, True], "latest policy")


def test_rc_candidate() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        repo, _ = init_repo(Path(tmp))
        sha = commit(repo, "feature-rc.txt", "rc\n", "feat: stage the next patch candidate")
        fixture_path = Path(tmp) / "fixtures.json"
        fixture_path.write_text(
            json.dumps(
                {
                    "commits_pulls": {sha: [{"number": 71}]},
                    "closed_pulls": [],
                    "issues": {
                        "71": ["type:patch", "channel:rc"],
                    },
                }
            )
        )
        payload = plan(repo, fixture_path, sha)
        assert_equal(payload["release_count"], 1, "rc release count")
        entry = payload["entries"][0]
        assert_equal(entry["version"], "0.8.2", "rc next version")
        assert_equal(entry["tag"], f"v0.8.2-rc.{sha[:7]}", "rc tag")
        assert_equal(entry["prerelease"], True, "rc prerelease flag")
        assert_equal(entry["publish_latest"], False, "rc latest policy")


def test_merge_commit_sha_inspect() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        repo, _ = init_repo(Path(tmp))
        sha = commit(repo, "feature-merge.txt", "merge\n", "feat(products): add manual region refresh")
        fixture_path = Path(tmp) / "fixtures.json"
        fixture_path.write_text(
            json.dumps(
                {
                    "commits_pulls": {},
                    "closed_pulls": [
                        {
                            "number": 70,
                            "merge_commit_sha": sha,
                            "merged_at": "2026-03-15T03:43:00Z",
                            "base": {"ref": "main"},
                        }
                    ],
                    "issues": {
                        "70": ["type:minor"],
                    },
                }
            )
        )
        payload = inspect(repo, fixture_path, sha)
        assert_equal(payload["pr_number"], 70, "merge fallback pr number")
        assert_equal(payload["resolution_source"], "merge_commit_sha", "merge fallback source")
        assert_equal(payload["channel_source"], "legacy-default", "legacy channel source")
        assert_equal(payload["should_release"], True, "merge fallback should release")


def test_unresolved_subject_skips() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        repo, _ = init_repo(Path(tmp))
        sha = commit(repo, "feature-direct.txt", "direct\n", "feat: direct push without pr mapping")
        fixture_path = Path(tmp) / "fixtures.json"
        fixture_path.write_text(json.dumps({"commits_pulls": {}, "closed_pulls": [], "issues": {}}))
        payload = inspect(repo, fixture_path, sha)
        assert_equal(payload["should_release"], False, "unresolved commit should skip")
        assert_equal(payload["resolution_source"], "unresolved", "unresolved source")
        assert_equal(payload["pr_number"], None, "unresolved pr number")



def test_commits_api_filters_non_main_pulls() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        repo, _ = init_repo(Path(tmp))
        sha = commit(repo, "feature-filter.txt", "filter\n", "feat: only the merged main pull should count")
        fixture_path = Path(tmp) / "fixtures.json"
        fixture_path.write_text(
            json.dumps(
                {
                    "commits_pulls": {
                        sha: [
                            {
                                "number": 10,
                                "merged_at": "2026-03-15T08:00:00Z",
                                "base": {"ref": "main"},
                            },
                            {
                                "number": 20,
                                "state": "open",
                                "base": {"ref": "release/0.8"},
                            },
                        ]
                    },
                    "closed_pulls": [],
                    "issues": {
                        "10": ["type:patch", "channel:stable"],
                    },
                }
            )
        )
        payload = inspect(repo, fixture_path, sha)
        assert_equal(payload["pr_number"], 10, "commits api should ignore unrelated pulls")
        assert_equal(payload["resolution_source"], "commits_api", "commits api resolution source")
        assert_equal(payload["should_release"], True, "filtered commits api release")

def main() -> int:
    test_sequential_stable_reconciliation()
    test_rc_candidate()
    test_merge_commit_sha_inspect()
    test_unresolved_subject_skips()
    test_commits_api_filters_non_main_pulls()
    print("test-release-plan: all cases passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
