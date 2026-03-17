#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import mimetypes
import sys
from pathlib import Path
from urllib import error, parse, request


class ReleasePublishError(RuntimeError):
    pass


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Create or update a GitHub release and upload release assets.")
    parser.add_argument("--repository", required=True, help="owner/repo")
    parser.add_argument("--token", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--target-sha", required=True)
    parser.add_argument("--name", required=True)
    parser.add_argument("--prerelease", choices=("true", "false"), required=True)
    parser.add_argument("--artifacts-dir", required=True)
    parser.add_argument("--api-root", default="https://api.github.com")
    parser.add_argument("--upload-root", default="https://uploads.github.com")
    return parser.parse_args()


def request_json(
    *,
    method: str,
    url: str,
    token: str,
    payload: dict[str, object] | None = None,
) -> tuple[dict[str, object], dict[str, str]]:
    headers = {
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28",
        "User-Agent": "catnap-release-publisher",
    }
    data = None
    if payload is not None:
        headers["Content-Type"] = "application/json"
        data = json.dumps(payload).encode("utf-8")
    req = request.Request(url, data=data, headers=headers, method=method)
    try:
        with request.urlopen(req) as resp:
            body = resp.read().decode("utf-8")
            parsed = json.loads(body) if body else {}
            return parsed, dict(resp.headers)
    except error.HTTPError as exc:
        body = exc.read().decode("utf-8", errors="replace")
        accepted = exc.headers.get("x-accepted-github-permissions", "(not provided)")
        raise ReleasePublishError(
            f"GitHub API {method} {url} failed: status={exc.code}, "
            f"accepted_permissions={accepted}, body={body}"
        ) from exc


def request_bytes(*, method: str, url: str, token: str, data: bytes, content_type: str) -> dict[str, object]:
    headers = {
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github+json",
        "Content-Type": content_type,
        "X-GitHub-Api-Version": "2022-11-28",
        "User-Agent": "catnap-release-publisher",
    }
    req = request.Request(url, data=data, headers=headers, method=method)
    try:
        with request.urlopen(req) as resp:
            body = resp.read().decode("utf-8")
            return json.loads(body) if body else {}
    except error.HTTPError as exc:
        body = exc.read().decode("utf-8", errors="replace")
        accepted = exc.headers.get("x-accepted-github-permissions", "(not provided)")
        raise ReleasePublishError(
            f"GitHub uploads API {method} {url} failed: status={exc.code}, "
            f"accepted_permissions={accepted}, body={body}"
        ) from exc


def release_by_tag(api_root: str, repository: str, token: str, tag: str) -> dict[str, object] | None:
    owner, repo = repository.split("/", 1)
    url = f"{api_root.rstrip('/')}/repos/{owner}/{repo}/releases/tags/{parse.quote(tag)}"
    try:
        payload, _ = request_json(method="GET", url=url, token=token)
    except ReleasePublishError as exc:
        message = str(exc)
        if "status=404" in message:
            return None
        raise
    return payload


def create_or_update_release(args: argparse.Namespace) -> dict[str, object]:
    owner, repo = args.repository.split("/", 1)
    base_url = f"{args.api_root.rstrip('/')}/repos/{owner}/{repo}/releases"
    existing = release_by_tag(args.api_root, args.repository, args.token, args.tag)
    prerelease = args.prerelease == "true"
    if existing is None:
        payload, _ = request_json(
            method="POST",
            url=base_url,
            token=args.token,
            payload={
                "tag_name": args.tag,
                "target_commitish": args.target_sha,
                "name": args.name,
                "prerelease": prerelease,
                "generate_release_notes": True,
                "make_latest": "legacy",
            },
        )
        return payload

    release_id = existing.get("id")
    if not isinstance(release_id, int):
        raise ReleasePublishError("Existing release is missing numeric id")
    payload, _ = request_json(
        method="PATCH",
        url=f"{base_url}/{release_id}",
        token=args.token,
        payload={
            "tag_name": args.tag,
            "target_commitish": args.target_sha,
            "name": args.name,
            "prerelease": prerelease,
            "make_latest": "legacy",
        },
    )
    return payload


def artifact_paths(artifacts_dir: str) -> list[Path]:
    root = Path(artifacts_dir)
    if not root.is_dir():
        raise ReleasePublishError(f"Artifacts directory does not exist: {root}")
    files = sorted(path for path in root.iterdir() if path.is_file())
    if not files:
        raise ReleasePublishError(f"No release assets found under {root}")
    return files


def delete_existing_assets(release: dict[str, object], args: argparse.Namespace, filenames: set[str]) -> None:
    owner, repo = args.repository.split("/", 1)
    assets = release.get("assets")
    if not isinstance(assets, list):
        return
    for asset in assets:
        if not isinstance(asset, dict):
            continue
        name = asset.get("name")
        asset_id = asset.get("id")
        if not isinstance(name, str) or name not in filenames or not isinstance(asset_id, int):
            continue
        request_json(
            method="DELETE",
            url=f"{args.api_root.rstrip('/')}/repos/{owner}/{repo}/releases/assets/{asset_id}",
            token=args.token,
        )


def upload_assets(release: dict[str, object], args: argparse.Namespace) -> None:
    upload_url = release.get("upload_url")
    if not isinstance(upload_url, str) or not upload_url:
        raise ReleasePublishError("Release payload is missing upload_url")
    upload_base = upload_url.split("{", 1)[0]
    files = artifact_paths(args.artifacts_dir)
    delete_existing_assets(release, args, {path.name for path in files})
    for path in files:
        content_type = mimetypes.guess_type(path.name)[0] or "application/octet-stream"
        query = parse.urlencode({"name": path.name})
        request_bytes(
            method="POST",
            url=f"{upload_base}?{query}",
            token=args.token,
            data=path.read_bytes(),
            content_type=content_type,
        )


def main() -> int:
    args = parse_args()
    try:
        release = create_or_update_release(args)
        upload_assets(release, args)
    except ReleasePublishError as exc:
        print(f"publish_github_release.py: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
