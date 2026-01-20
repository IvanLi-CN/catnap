# File / Config Contracts（#0003）

本文件定义发版相关的“可持续约定”（tags、镜像命名、GitHub Actions 触发与权限、release assets），用于让 CI/CD 的行为稳定且可验证。

## Git tag & GitHub Release naming

- Tag format: `v<semver>`
  - Example: `v0.1.0`
- Release:
  - Release name 与 tag 对齐（例如 `v0.1.0`）
  - Release notes：默认使用 GitHub 自动生成（`generate_release_notes: true`）

## GHCR image naming & tagging

### Single image (default)

- App image: `ghcr.io/<owner>/catnap`
- Tags (minimum):
  - `v<semver>`（例如 `v0.1.0`）
  - `latest`（仅默认分支 / `main`）

### Optional tags (non-contract)

- 允许额外 tag（例如 `sha`）用于排障，但不得替代 `v<semver>` 与 `latest` 的最低集合。

## GitHub Actions workflow contract (release-related)

### Triggers

- `pull_request`（to `main`）：执行 lint/tests + 发布链路构建校验（不 push）
- `push`（to `main`）：执行 lint/tests + 自动打 tag/创建 Release + 上传 Release assets + 推送 GHCR 镜像（含 `latest`）
- `workflow_dispatch`（required）：用于“手动发布/兜底”触发（与自动发布同一条发布链路）

### `workflow_dispatch` semantics (frozen)

- 支持选择 ref：
  - ref=`main`：行为与自动发布一致（创建 tag/release、上传 assets、推送 `v<semver>` + 更新 `latest`）
  - ref=`refs/tags/v<semver>`：用于重跑/补齐同一版本（上传 assets、推送 `v<semver>`；不更新 `latest`）

### Required permissions

- Default: `contents: read`
- Release steps (tag/release): `contents: write`
- GHCR push: `packages: write`

## Dockerfile paths

- Target Dockerfile: `Dockerfile`（repo root）

## GitHub Release assets

Release 建议附带可复用的二进制产物（assets）。本计划默认以 tarball 形态发布，并附带 sha256 校验和。

> 目标矩阵已冻结：linux/amd64+arm64，gnu+musl（共 4 个 tar.gz + 4 个 sha256）。

### Asset naming (proposed)

- `catnap_<semver>_linux_amd64_gnu.tar.gz`
- `catnap_<semver>_linux_arm64_gnu.tar.gz`
- `catnap_<semver>_linux_amd64_musl.tar.gz`
- `catnap_<semver>_linux_arm64_musl.tar.gz`
- `catnap_<semver>_linux_amd64_gnu.tar.gz.sha256`
- `catnap_<semver>_linux_arm64_gnu.tar.gz.sha256`
- `catnap_<semver>_linux_amd64_musl.tar.gz.sha256`
- `catnap_<semver>_linux_arm64_musl.tar.gz.sha256`

### Asset contents (proposed)

每个 tar.gz 至少包含：

- `catnap`（server binary）

> Web UI 静态资源已冻结为 embed 到二进制，因此 assets 不再需要包含 `web/dist/`。

### Idempotency

- 同一 tag 的 Release 上重复运行上传步骤时，workflow 必须具备幂等策略（固定为“替换式上传”）：
  - 使用支持 `allowUpdates`/`replacesArtifacts` 等价能力的 release 上传 action（实现阶段冻结具体 action）

## Version metadata (required)

### OCI labels (Docker image)

- `org.opencontainers.image.version=<semver>`
- `org.opencontainers.image.revision=<git-sha>`
- `org.opencontainers.image.source=https://github.com/<owner>/<repo>`

### Runtime env

- `APP_EFFECTIVE_VERSION=<semver>`

### HTTP API alignment

- `/api/health` 的 `version` 字段必须与 `APP_EFFECTIVE_VERSION` 一致（见 `contracts/http-apis.md`）。

## Toolchain pinning（major）

本计划已冻结：发布链路依赖版本以“major 级别 pinning”为主，避免 `latest` 漂移导致不可复现。

（Checked: 2026-01-20）

### GitHub Actions (major pins)

- `actions/checkout@v6`（latest release: `v6.0.1`）
- `oven-sh/setup-bun@v2`（latest release: `v2.1.2`；`bun-version: 1`，bun latest: `bun-v1.3.6`）
- `dtolnay/rust-toolchain@v1`（Rust stable: `1.92.0`）
- `docker/setup-qemu-action@v3`
- `docker/setup-buildx-action@v3`
- `docker/login-action@v3`
- `docker/metadata-action@v5`
- `docker/build-push-action@v6`
- `softprops/action-gh-release@v2`

### Dockerfile base images (major pins)

- `oven/bun:1`（web build）
- `rust:1`（backend build）
- `debian:13-slim`（runtime；Debian 13 / trixie）
