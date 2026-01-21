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
- `push`（to `main`）：默认执行 lint/tests；仅当合并 PR 的标签为 `type:major|minor|patch` 时才允许进入“自动发版链路”（打 tag/创建 Release/上传 assets/推送 GHCR，含 `latest`）
- `workflow_dispatch`（required）：用于“手动发布/兜底”触发（与自动发布同一条发布链路）

### Release intent (PR labels) (frozen)

本节定义“哪些 PR 允许触发自动发版”，用于避免仅文档/设计类变更也触发新版本迭代。

- Source of truth: PR labels（CI 在 PR 阶段必须校验并 enforce）
- Requirement:
  - PR 必须且只能包含一个“意图标签”（mutually exclusive）
  - 合并到 `main` 后，发布链路仅对“允许发版”的意图标签生效
- Label set (mutually exclusive, exactly one required):
  - `type:docs`：文档/设计类变更；不允许自动发版
  - `type:skip`：显式跳过自动发版（不论变更内容为何）；不允许自动发版
  - `type:patch`：允许自动发版；CI 做 patch bump 并发布
  - `type:minor`：允许自动发版；CI 做 minor bump 并发布
  - `type:major`：允许自动发版；CI 做 major bump 并发布

> 已选择方案 C：以 PR 标签作为发布意图的唯一信号。对外可观察行为必须一致：`type:docs` 的合并不得创建新 tag/Release，也不得推送新的 `v<semver>` 镜像 tag。

### PR label gate (方案 C contract)

- PR 阶段：
  - Gate job: `label-gate`（name 可调整，但行为契约需保持）
  - Behavior:
    - 若 PR 缺少意图标签：必须失败（提示如何打标签）
    - 若 PR 同时存在多个意图标签：必须失败（提示互斥规则）
    - 若 PR 标签有效：通过
- `push main` 阶段（自动发版 gating）：
  - Gate job: `release-intent`（name 可调整，但输出契约需保持）
  - Output:
    - `should_release`：`true|false`
    - `bump_level`：`major|minor|patch|none`
  - Semantics:
    - `should_release=true`：允许进入发布链路（tag/release/assets/GHCR）；且 `bump_level` 必须为 `major|minor|patch`
    - `should_release=false`：必须跳过发布链路（允许继续执行 lint/tests）
    - Label mapping:
      - `type:major` → `should_release=true`, `bump_level=major`
      - `type:minor` → `should_release=true`, `bump_level=minor`
      - `type:patch` → `should_release=true`, `bump_level=patch`
      - `type:docs` / `type:skip` → `should_release=false`, `bump_level=none`

### Version bump rules (frozen)

本节定义“根据 PR 标签计算新版本号”的规则。

#### Base version selection（frozen）

Base version 为一个语义版本 `vX.Y.Z`，用于作为 bump 的起点。

- Option A (chosen): 从仓库现存 tags 中选取**语义版本最大值**作为 base（忽略非 `v<semver>` 的 tag）
  - Fallback: 若仓库尚无任何 `v<semver>` tags，则使用 `Cargo.toml` 的 `version` 作为 base

#### Bump math（frozen）

对 base `X.Y.Z` 应用 bump：

- `major`: `(X+1).0.0`
- `minor`: `X.(Y+1).0`
- `patch`: `X.Y.(Z+1)`

#### Uniqueness & retry（frozen）

- 目标 tag 为 `v<next>`。
- 若目标 tag 已存在：继续递增 patch 直到找到未占用版本（避免并发/重跑导致冲突）。

#### Examples（non-normative）

- base=`v0.1.4` + `type:patch` → `v0.1.5`
- base=`v0.1.4` + `type:minor` → `v0.2.0`
- base=`v0.1.4` + `type:major` → `v1.0.0`

### No PR / direct push policy (frozen)

当 `push main` 的提交无法关联到 PR 时，必须有可预测策略：

- `should_release=false`（跳过自动发版；需要时走 `workflow_dispatch` 手动发布）

### `workflow_dispatch` semantics (frozen)

手动发版用于兜底/重跑，必须可预测。

- 支持选择 ref：
  - ref=`main`：
    - 必须显式提供 `bump_level`（`major|minor|patch`）
    - 行为与自动发布一致（创建 tag/release、上传 assets、推送 `v<semver>` + 更新 `latest`）
  - ref=`refs/tags/v<semver>`：
    - 用于重跑/补齐同一版本（上传 assets、推送 `v<semver>`；不更新 `latest`）

### Release creation (required)

当 `should_release=true` 时，发布链路必须包含以下结果（顺序可调整，但可观察结果必须一致）：

1. 计算 `APP_EFFECTIVE_VERSION=<semver>`（见 `contracts/cli.md`）
2. Git tag：
   - 若 `refs/tags/v<APP_EFFECTIVE_VERSION>` 不存在：创建 annotated tag 并 push
   - 若已存在：必须采取明确策略（跳过或失败），不得留下半成品状态
3. GitHub Release：
   - Tag: `v<APP_EFFECTIVE_VERSION>`
   - Name: `v<APP_EFFECTIVE_VERSION>`
   - Release notes: `generate_release_notes: true`（默认）
4. Release assets 上传（tar.gz + sha256；见本文件 “GitHub Release assets”）
5. GHCR 推送：
   - 必须包含 `v<APP_EFFECTIVE_VERSION>`
   - 仅当 ref 为 `main` 时允许更新 `latest`

### Idempotency (required)

- Tag/release 创建必须幂等：workflow 重跑不应破坏仓库状态
- Assets 上传必须幂等：同一 tag 的 Release 上重复运行时，必须采取“替换式上传”（不得留下同名旧资产导致混乱）

### Required permissions

- Default: `contents: read`
- Release steps (tag/release): `contents: write`
- GHCR push: `packages: write`

## Dockerfile paths

- Target Dockerfile: `Dockerfile`（repo root）

## GitHub Release assets

Release 必须附带可复用的二进制产物（assets）。本计划默认以 tarball 形态发布，并附带 sha256 校验和。

> 目标矩阵已冻结：linux/amd64+arm64，gnu+musl（共 4 个 tar.gz + 4 个 sha256）。

### Asset naming (required)

- `catnap_<semver>_linux_amd64_gnu.tar.gz`
- `catnap_<semver>_linux_arm64_gnu.tar.gz`
- `catnap_<semver>_linux_amd64_musl.tar.gz`
- `catnap_<semver>_linux_arm64_musl.tar.gz`
- `catnap_<semver>_linux_amd64_gnu.tar.gz.sha256`
- `catnap_<semver>_linux_arm64_gnu.tar.gz.sha256`
- `catnap_<semver>_linux_amd64_musl.tar.gz.sha256`
- `catnap_<semver>_linux_arm64_musl.tar.gz.sha256`
以上命名为最低要求；不得更改平台/ABI 的编码方式，避免使用方脚本失效。

### Asset contents (required)

每个 tar.gz 至少包含：

- `catnap`（server binary）

### Checksum file format (required)

- `<asset>.sha256` 文件内容为一行：
  - `<hex_sha256>  <asset_filename>`
  - 例如：`e3b0c442...  catnap_0.1.5_linux_amd64_gnu.tar.gz`

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
