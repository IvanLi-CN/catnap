# CLI Contracts（0005）

本文件定义“发版意图标签 gate + 版本号计算”的 CI 脚本接口（面向 CI/维护者），用于稳定地产出 `APP_EFFECTIVE_VERSION` 并 gate 自动发版链路。

## `.github/scripts/label-gate.sh` (planned)

- Scope: internal
- Change: New

### Purpose

在 PR 阶段强制“意图标签互斥且必须 1 个”，并产出统一信号供后续 job 使用。

### Inputs

- `GITHUB_TOKEN`（required）
- `GITHUB_REPOSITORY`（required）
- `PR_NUMBER`（required）
- Allowed labels（required）：见 `contracts/file-formats.md` 的 `type:*` 集合

### Outputs

- `$GITHUB_OUTPUT`:
  - `intent_label=type:docs|type:skip|type:patch|type:minor|type:major`
  - `should_release=true|false`
  - `bump_level=major|minor|patch|none`

### Exit codes

- `0`: 校验通过
- `!= 0`: 校验失败（缺少意图标签 / 同时存在多个意图标签 / 存在未知意图标签）

## `.github/scripts/release-intent.sh` (planned)

- Scope: internal
- Change: New

### Purpose

在 `push main` 阶段将“commit”映射为发布意图（`should_release` + `bump_level`），用于 gate 自动发版链路。

### Inputs

- `GITHUB_TOKEN`（required）：用于查询 GitHub API（读取 commit 关联 PR 与 labels）
- `GITHUB_REPOSITORY`（required）
- `GITHUB_SHA`（required）
- Label contract（required）：见 `contracts/file-formats.md`

### Outputs

- `$GITHUB_OUTPUT`:
  - `should_release=true|false`
  - `bump_level=major|minor|patch|none`

### Behavior (normative)

- 若能关联到 PR：按 `contracts/file-formats.md` 的 label mapping 输出。
- 若无法关联到 PR：按 “No PR / direct push policy” 输出 `should_release=false`、`bump_level=none`。

### Exit codes

- `0`: 判定成功
- `!= 0`: 判定失败（仅允许在“应当失败”的策略下使用；默认推荐不因为 API 瞬时失败而误发版）

## `.github/scripts/compute-version.sh` (planned change)

- Scope: internal
- Change: Modify

### Purpose

根据 `BUMP_LEVEL` 与仓库 tags 计算有效版本号，并导出到 `APP_EFFECTIVE_VERSION`。

### Inputs

- `BUMP_LEVEL=major|minor|patch`（required）
- Git tags（`v<semver>`；用于选择 base version 与避免冲突）
- `Cargo.toml` version（用于无 tag fallback）

### Outputs

- `$GITHUB_ENV`: `APP_EFFECTIVE_VERSION=<semver>`

### Algorithm (normative)

1. Resolve base version：语义版本最大 tag；无 tag fallback `Cargo.toml`。
2. Apply bump math：按 `BUMP_LEVEL` 计算 `next`。
3. Ensure uniqueness：若 `v<next>` 已存在则继续递增 patch。
4. Export：写入 `APP_EFFECTIVE_VERSION=<next>`。

