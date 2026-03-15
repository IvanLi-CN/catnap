#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Any

API_VERSION = "2022-11-28"
ALLOWED_INTENT_LABELS = {
    "type:docs",
    "type:skip",
    "type:patch",
    "type:minor",
    "type:major",
}
ALLOWED_CHANNEL_LABELS = {
    "channel:stable",
    "channel:rc",
}


class PlanError(RuntimeError):
    pass


@dataclass(frozen=True)
class CommitRelease:
    commit_sha: str
    commit_subject: str
    pr_number: int
    intent_label: str
    channel_label: str
    channel_source: str
    should_release: bool
    bump_level: str
    resolution_source: str


class GitHubClient:
    def get_commit_pulls(self, commit_sha: str) -> list[dict[str, Any]]:
        raise NotImplementedError

    def get_closed_pulls(self) -> list[dict[str, Any]]:
        raise NotImplementedError

    def get_issue_labels(self, pull_number: int) -> list[str]:
        raise NotImplementedError


class ApiGitHubClient(GitHubClient):
    def __init__(self, owner: str, repo: str, api_root: str, token: str) -> None:
        self.owner = owner
        self.repo = repo
        self.api_root = api_root.rstrip("/")
        self.token = token
        self._closed_pulls: list[dict[str, Any]] | None = None

    def _request_json(
        self,
        path: str,
        query: dict[str, Any] | None = None,
        *,
        accept: str = "application/vnd.github+json",
    ) -> Any:
        url = self.api_root + path
        if query:
            url += "?" + urllib.parse.urlencode(query)
        headers = {
            "Accept": accept,
            "User-Agent": "catnap-release-plan/1.0",
            "X-GitHub-Api-Version": API_VERSION,
        }
        if self.token:
            headers["Authorization"] = f"Bearer {self.token}"
        request = urllib.request.Request(url, headers=headers)
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                return json.load(response)
        except urllib.error.HTTPError as exc:
            detail = exc.read().decode("utf-8", errors="replace")
            raise PlanError(f"GitHub API request failed ({exc.code}) for {path}: {detail or exc.reason}") from exc
        except urllib.error.URLError as exc:
            raise PlanError(f"GitHub API request failed for {path}: {exc.reason}") from exc

    def _paginate(
        self,
        path: str,
        query: dict[str, Any] | None = None,
        *,
        accept: str = "application/vnd.github+json",
    ) -> list[dict[str, Any]]:
        items: list[dict[str, Any]] = []
        page = 1
        while True:
            payload = self._request_json(path, {**(query or {}), "per_page": 100, "page": page}, accept=accept)
            if not isinstance(payload, list):
                raise PlanError(f"Expected list payload from {path}, got {type(payload).__name__}")
            items.extend(item for item in payload if isinstance(item, dict))
            if len(payload) < 100:
                break
            page += 1
        return items

    def get_commit_pulls(self, commit_sha: str) -> list[dict[str, Any]]:
        path = f"/repos/{self.owner}/{self.repo}/commits/{urllib.parse.quote(commit_sha, safe='')}/pulls"
        try:
            return self._paginate(path)
        except PlanError as exc:
            if "(404)" not in str(exc):
                raise
            return self._paginate(path, accept="application/vnd.github.groot-preview+json")

    def get_closed_pulls(self) -> list[dict[str, Any]]:
        if self._closed_pulls is None:
            self._closed_pulls = self._paginate(
                f"/repos/{self.owner}/{self.repo}/pulls",
                {"state": "closed", "sort": "updated", "direction": "desc"},
            )
        return self._closed_pulls

    def get_issue_labels(self, pull_number: int) -> list[str]:
        payload = self._request_json(f"/repos/{self.owner}/{self.repo}/issues/{pull_number}")
        labels = payload.get("labels") or []
        if not isinstance(labels, list):
            raise PlanError(f"Issue labels for PR #{pull_number} must be a list")
        names = [str(label.get("name")) for label in labels if isinstance(label, dict) and label.get("name")]
        return sorted(set(names))


class FixtureGitHubClient(GitHubClient):
    def __init__(self, fixture_path: str) -> None:
        payload = json.loads(Path(fixture_path).read_text())
        if not isinstance(payload, dict):
            raise PlanError("fixtures payload must be a JSON object")
        self.commit_pulls = payload.get("commits_pulls", {})
        self.closed_pulls = payload.get("closed_pulls", [])
        self.issue_labels = payload.get("issues", {})

    def get_commit_pulls(self, commit_sha: str) -> list[dict[str, Any]]:
        payload = self.commit_pulls.get(commit_sha, [])
        if not isinstance(payload, list):
            raise PlanError(f"fixtures.commits_pulls[{commit_sha}] must be a list")
        return [item for item in payload if isinstance(item, dict)]

    def get_closed_pulls(self) -> list[dict[str, Any]]:
        if not isinstance(self.closed_pulls, list):
            raise PlanError("fixtures.closed_pulls must be a list")
        return [item for item in self.closed_pulls if isinstance(item, dict)]

    def get_issue_labels(self, pull_number: int) -> list[str]:
        payload = self.issue_labels.get(str(pull_number), [])
        if not isinstance(payload, list):
            raise PlanError(f"fixtures.issues[{pull_number}] must be a list")
        return sorted({str(label) for label in payload if label})


@dataclass(frozen=True)
class RepoContext:
    repo_root: Path
    owner: str
    repo: str
    base_branch: str


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Plan and inspect catnap release candidates.")
    subparsers = parser.add_subparsers(dest="command", required=True)

    inspect_parser = subparsers.add_parser("inspect-commit", help="Inspect one commit and classify its release intent.")
    add_common_args(inspect_parser, require_target_sha=True)
    inspect_parser.add_argument("--commit-subject", default="", help="Override commit subject for tests.")

    plan_parser = subparsers.add_parser("plan", help="Plan missing releases up to a target SHA.")
    add_common_args(plan_parser, require_target_sha=True)
    plan_parser.add_argument("--target-ref", default="main", help="Human-readable target ref for summaries.")

    return parser.parse_args()


def add_common_args(parser: argparse.ArgumentParser, *, require_target_sha: bool) -> None:
    parser.add_argument("--repo", default=os.environ.get("GITHUB_REPOSITORY", ""), help="Repository in owner/name form.")
    parser.add_argument("--repo-root", default=".", help="Repository root for git history inspection.")
    parser.add_argument("--api-root", default=os.environ.get("GITHUB_API_URL", "https://api.github.com"))
    parser.add_argument("--token", default=os.environ.get("GITHUB_TOKEN", ""))
    parser.add_argument("--fixtures", default=os.environ.get("TEST_RELEASE_PLAN_FIXTURES", ""))
    parser.add_argument("--base-branch", default="main")
    parser.add_argument(
        "--legacy-missing-channel",
        choices=("stable", "error"),
        default=os.environ.get("RELEASE_LEGACY_MISSING_CHANNEL", "stable"),
        help="How to classify historical merged PRs without channel labels.",
    )
    parser.add_argument("--github-output", default=os.environ.get("GITHUB_OUTPUT", ""))
    parser.add_argument("--target-sha", required=require_target_sha, default=os.environ.get("GITHUB_SHA", ""))


def build_repo_context(args: argparse.Namespace) -> RepoContext:
    owner, sep, repo = args.repo.partition("/")
    if not sep or not owner or not repo:
        raise PlanError("--repo must be in owner/name form")
    repo_root = Path(args.repo_root).resolve()
    if not repo_root.exists():
        raise PlanError(f"repo root does not exist: {repo_root}")
    return RepoContext(repo_root=repo_root, owner=owner, repo=repo, base_branch=args.base_branch)


def build_client(args: argparse.Namespace, context: RepoContext) -> GitHubClient:
    if args.fixtures:
        return FixtureGitHubClient(args.fixtures)
    if not args.token:
        raise PlanError("GITHUB_TOKEN or --token is required unless --fixtures is provided")
    return ApiGitHubClient(context.owner, context.repo, args.api_root, args.token)


def run_git(repo_root: Path, *argv: str, check: bool = True) -> str:
    result = subprocess.run(
        ["git", *argv],
        cwd=repo_root,
        check=False,
        text=True,
        capture_output=True,
    )
    if check and result.returncode != 0:
        raise PlanError(f"git {' '.join(argv)} failed: {result.stderr.strip() or result.stdout.strip()}")
    return result.stdout.strip()


def git_subject(repo_root: Path, commit_sha: str, override: str = "") -> str:
    if override:
        return override
    return run_git(repo_root, "log", "-1", "--format=%s", commit_sha)


def git_first_parent_commits(repo_root: Path, target_sha: str, since_ref: str | None) -> list[str]:
    range_spec = f"{since_ref}..{target_sha}" if since_ref else target_sha
    output = run_git(repo_root, "rev-list", "--first-parent", "--reverse", range_spec)
    return [line for line in output.splitlines() if line]


def git_latest_stable_tag(repo_root: Path, target_sha: str) -> tuple[str | None, str | None]:
    merged_tags = run_git(repo_root, "tag", "--merged", target_sha, "-l", "v[0-9]*.[0-9]*.[0-9]*", check=False)
    tags = [line.strip() for line in merged_tags.splitlines() if line.strip()]
    if not tags:
        return None, None
    latest_tag = sorted(tags, key=version_sort_key)[-1]
    return latest_tag, latest_tag[1:]


def version_sort_key(tag: str) -> tuple[int, int, int]:
    version = tag[1:] if tag.startswith("v") else tag
    major, minor, patch = version.split(".")
    return int(major), int(minor), int(patch)


def bump_version(version: str, bump_level: str) -> str:
    major_s, minor_s, patch_s = version.split(".")
    major, minor, patch = int(major_s), int(minor_s), int(patch_s)
    if bump_level == "major":
        return f"{major + 1}.0.0"
    if bump_level == "minor":
        return f"{major}.{minor + 1}.0"
    if bump_level == "patch":
        return f"{major}.{minor}.{patch + 1}"
    raise PlanError(f"unsupported bump level: {bump_level}")


def parse_pull_number_from_subject(subject: str) -> int | None:
    if subject.startswith("Merge pull request #"):
        tail = subject[len("Merge pull request #") :]
        number = ""
        for ch in tail:
            if ch.isdigit():
                number += ch
                continue
            break
        if number:
            return int(number)
    if subject.endswith(")") and "(#" in subject:
        start = subject.rfind("(#")
        maybe = subject[start + 2 : -1]
        if maybe.isdigit():
            return int(maybe)
    return None


def normalize_ref(ref: str) -> str:
    if ref.startswith("refs/heads/"):
        return ref[len("refs/heads/") :]
    return ref


def resolve_single_pull(
    client: GitHubClient,
    commit_sha: str,
    base_branch: str,
    commit_subject: str,
) -> tuple[int | None, str]:
    pulls = [item for item in client.get_commit_pulls(commit_sha) if isinstance(item.get("number"), int)]
    if len(pulls) == 1:
        return int(pulls[0]["number"]), "commits_api"
    if len(pulls) > 1:
        raise PlanError(f"commit {commit_sha} resolved to multiple PRs via commits API")

    merge_commit_matches: list[int] = []
    for pull in client.get_closed_pulls():
        if pull.get("merge_commit_sha") != commit_sha:
            continue
        if not pull.get("merged_at"):
            continue
        base = pull.get("base") or {}
        if normalize_ref(str(base.get("ref", ""))) != base_branch:
            continue
        number = pull.get("number")
        if isinstance(number, int) and number > 0:
            merge_commit_matches.append(number)
    merge_commit_matches = sorted(set(merge_commit_matches))
    if len(merge_commit_matches) == 1:
        return merge_commit_matches[0], "merge_commit_sha"
    if len(merge_commit_matches) > 1:
        raise PlanError(f"commit {commit_sha} resolved to multiple PRs via merge_commit_sha fallback")

    subject_pull = parse_pull_number_from_subject(commit_subject)
    if subject_pull is not None:
        return subject_pull, "subject_fallback"
    return None, "unresolved"


def classify_labels(labels: list[str], legacy_missing_channel: str) -> tuple[str, str, str, bool, str]:
    type_labels = sorted({label for label in labels if label.startswith("type:")})
    channel_labels = sorted({label for label in labels if label.startswith("channel:")})

    unknown_type_labels = [label for label in type_labels if label not in ALLOWED_INTENT_LABELS]
    unknown_channel_labels = [label for label in channel_labels if label not in ALLOWED_CHANNEL_LABELS]
    if unknown_type_labels:
        raise PlanError(f"unknown type labels: {', '.join(unknown_type_labels)}")
    if unknown_channel_labels:
        raise PlanError(f"unknown channel labels: {', '.join(unknown_channel_labels)}")
    if len(type_labels) != 1:
        raise PlanError(f"expected exactly one type:* label, found {len(type_labels)}")

    if len(channel_labels) == 0:
        if legacy_missing_channel == "stable":
            channel_label = "channel:stable"
            channel_source = "legacy-default"
        else:
            raise PlanError("expected exactly one channel:* label, found 0")
    elif len(channel_labels) == 1:
        channel_label = channel_labels[0]
        channel_source = "label"
    else:
        raise PlanError(f"expected exactly one channel:* label, found {len(channel_labels)}")

    intent_label = type_labels[0]
    if intent_label in {"type:docs", "type:skip"}:
        return intent_label, channel_label, channel_source, False, "none"
    bump_level = intent_label.split(":", 1)[1]
    return intent_label, channel_label, channel_source, True, bump_level


def inspect_commit(
    context: RepoContext,
    client: GitHubClient,
    commit_sha: str,
    legacy_missing_channel: str,
    commit_subject_override: str = "",
) -> CommitRelease:
    subject = git_subject(context.repo_root, commit_sha, commit_subject_override)
    pr_number, resolution_source = resolve_single_pull(client, commit_sha, context.base_branch, subject)
    if pr_number is None:
        return CommitRelease(
            commit_sha=commit_sha,
            commit_subject=subject,
            pr_number=0,
            intent_label="none",
            channel_label="none",
            channel_source="none",
            should_release=False,
            bump_level="none",
            resolution_source=resolution_source,
        )

    labels = client.get_issue_labels(pr_number)
    intent_label, channel_label, channel_source, should_release, bump_level = classify_labels(labels, legacy_missing_channel)
    return CommitRelease(
        commit_sha=commit_sha,
        commit_subject=subject,
        pr_number=pr_number,
        intent_label=intent_label,
        channel_label=channel_label,
        channel_source=channel_source,
        should_release=should_release,
        bump_level=bump_level,
        resolution_source=resolution_source,
    )


def build_plan(
    context: RepoContext,
    client: GitHubClient,
    target_sha: str,
    target_ref: str,
    legacy_missing_channel: str,
) -> dict[str, Any]:
    latest_stable_tag, latest_stable_version = git_latest_stable_tag(context.repo_root, target_sha)
    current_stable = latest_stable_version or cargo_version(context.repo_root)
    first_parent_commits = git_first_parent_commits(context.repo_root, target_sha, latest_stable_tag)

    entries: list[dict[str, Any]] = []
    skipped: list[dict[str, Any]] = []
    for commit_sha in first_parent_commits:
        commit_info = inspect_commit(context, client, commit_sha, legacy_missing_channel)
        if not commit_info.should_release:
            skipped.append(
                {
                    "commit_sha": commit_info.commit_sha,
                    "commit_subject": commit_info.commit_subject,
                    "pr_number": commit_info.pr_number or None,
                    "intent_label": commit_info.intent_label,
                    "channel_label": commit_info.channel_label,
                    "channel_source": commit_info.channel_source,
                    "resolution_source": commit_info.resolution_source,
                }
            )
            continue

        version = bump_version(current_stable, commit_info.bump_level)
        channel = commit_info.channel_label.split(":", 1)[1]
        if channel == "stable":
            app_version = version
            tag = f"v{version}"
            current_stable = version
        elif channel == "rc":
            app_version = f"{version}-rc.{commit_info.commit_sha[:7]}"
            tag = f"v{app_version}"
        else:
            raise PlanError(f"unsupported channel: {channel}")

        entries.append(
            {
                "commit_sha": commit_info.commit_sha,
                "commit_subject": commit_info.commit_subject,
                "pr_number": commit_info.pr_number,
                "intent_label": commit_info.intent_label,
                "channel_label": commit_info.channel_label,
                "channel_source": commit_info.channel_source,
                "channel": channel,
                "bump_level": commit_info.bump_level,
                "resolution_source": commit_info.resolution_source,
                "version": version,
                "app_version": app_version,
                "tag": tag,
                "prerelease": channel == "rc",
                "publish_latest": False,
                "tag_exists": git_ref_exists(context.repo_root, tag),
            }
        )

    stable_indexes = [index for index, entry in enumerate(entries) if entry["channel"] == "stable"]
    if stable_indexes:
        entries[stable_indexes[-1]]["publish_latest"] = True

    return {
        "target_sha": target_sha,
        "target_ref": target_ref,
        "base_branch": context.base_branch,
        "base_stable_tag": latest_stable_tag,
        "base_stable_version": latest_stable_version or cargo_version(context.repo_root),
        "entries": entries,
        "skipped_commits": skipped,
        "release_count": len(entries),
    }


def cargo_version(repo_root: Path) -> str:
    cargo_toml = repo_root / "Cargo.toml"
    for line in cargo_toml.read_text().splitlines():
        line = line.strip()
        if line.startswith("version") and "=" in line:
            return line.split("=", 1)[1].strip().strip('"')
    raise PlanError("failed to detect version from Cargo.toml")


def git_ref_exists(repo_root: Path, tag: str) -> bool:
    result = subprocess.run(
        ["git", "rev-parse", "-q", "--verify", f"refs/tags/{tag}"],
        cwd=repo_root,
        check=False,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    return result.returncode == 0


def write_output(path: str, key: str, value: str) -> None:
    if not path:
        return
    with open(path, "a", encoding="utf-8") as handle:
        handle.write(f"{key}={value}\n")


def write_multiline_output(path: str, key: str, value: str) -> None:
    if not path:
        return
    with open(path, "a", encoding="utf-8") as handle:
        handle.write(f"{key}<<EOF\n{value}\nEOF\n")


def handle_inspect(args: argparse.Namespace) -> int:
    context = build_repo_context(args)
    client = build_client(args, context)
    info = inspect_commit(
        context,
        client,
        args.target_sha,
        args.legacy_missing_channel,
        commit_subject_override=args.commit_subject,
    )
    payload = {
        "bump_level": info.bump_level,
        "channel_label": info.channel_label,
        "channel_source": info.channel_source,
        "commit_sha": info.commit_sha,
        "commit_subject": info.commit_subject,
        "intent_label": info.intent_label,
        "pr_number": None if info.pr_number == 0 else info.pr_number,
        "resolution_source": info.resolution_source,
        "should_release": info.should_release,
    }
    print(json.dumps(payload, indent=2, sort_keys=True))
    write_output(args.github_output, "should_release", "true" if info.should_release else "false")
    write_output(args.github_output, "bump_level", info.bump_level)
    write_output(args.github_output, "intent_label", info.intent_label)
    write_output(args.github_output, "channel_label", info.channel_label)
    write_output(args.github_output, "pr_number", str(info.pr_number) if info.pr_number else "none")
    write_output(args.github_output, "resolution_source", info.resolution_source)
    write_output(args.github_output, "channel_source", info.channel_source)
    return 0


def handle_plan(args: argparse.Namespace) -> int:
    context = build_repo_context(args)
    client = build_client(args, context)
    plan = build_plan(context, client, args.target_sha, args.target_ref, args.legacy_missing_channel)
    print(json.dumps(plan, indent=2, sort_keys=True))
    write_output(args.github_output, "has_releases", "true" if plan["entries"] else "false")
    write_output(args.github_output, "release_count", str(plan["release_count"]))
    write_multiline_output(args.github_output, "release_matrix", json.dumps({"include": plan["entries"]}, separators=(",", ":")))
    write_multiline_output(args.github_output, "release_plan", json.dumps(plan, separators=(",", ":")))
    return 0


def main() -> int:
    args = parse_args()
    try:
        if args.command == "inspect-commit":
            return handle_inspect(args)
        return handle_plan(args)
    except PlanError as exc:
        print(f"release-plan: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
