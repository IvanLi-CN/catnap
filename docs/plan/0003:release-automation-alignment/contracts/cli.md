# CLI Contracts（#0003）

本文件定义“发版链路”中被 CI 调用的脚本接口（面向 CI/维护者），用于稳定地产出版本号口径，并对构建产物做最小运行验证。

## `.github/scripts/compute-version.sh`

- Scope: internal
- Change: Modify

### Purpose

计算本次发布的有效版本号，并导出到环境变量 `APP_EFFECTIVE_VERSION`。

### Inputs

- PR label gate 产出的 bump level（required）：
  - `BUMP_LEVEL=major|minor|patch`
- Base version inputs（required；具体选择规则见 `contracts/file-formats.md` 的 “Base version selection”）：
  - 已存在的 git tags（形如 `v<semver>`；用于找到 base version 与避免冲突）
  - `Cargo.toml` 中的 `version`（形如 `x.y.z`；用于 fallback 或作为 base，取决于最终冻结策略）

### Outputs

- 向 `$GITHUB_ENV` 写入：`APP_EFFECTIVE_VERSION=<semver>`
- stdout 打印（至少）base/bump/target tag 的关键日志（用于排障）
- stdout 打印 computed version（用于日志）

### Exit codes

- `0`: 计算成功
- `!= 0`: 计算失败（例如无法读取 `Cargo.toml` 版本号）

### Algorithm (normative)

1. Resolve base version `X.Y.Z`：
   - 按 `contracts/file-formats.md` 的 “Base version selection” 冻结策略执行（tags 或 Cargo.toml；tags 下无 tag 时可 fallback Cargo.toml）。
2. Apply bump math（由 `BUMP_LEVEL` 决定）：
   - `major`: `(X+1).0.0`
   - `minor`: `X.(Y+1).0`
   - `patch`: `X.Y.(Z+1)`
3. Ensure uniqueness：
   - 若 `refs/tags/v<next>` 已存在，则持续递增 patch 直到找到未占用版本。
4. Export：
   - `APP_EFFECTIVE_VERSION=<next>` 写入 `$GITHUB_ENV`。

## `.github/scripts/smoke-test.sh` (planned)

- Scope: internal
- Change: New

### Purpose

在 CI 中对“构建产物”做最小运行验证（启动服务端→探活→访问 UI）。

### Inputs

- `CATNAP_SMOKE_BIN` (required): 要测试的二进制路径（例如 `./target/release/catnap`）
- `CATNAP_SMOKE_ADDR` (optional, default `127.0.0.1:18080`): 服务监听地址
- `CATNAP_SMOKE_TIMEOUT_SECONDS` (optional, default `20`): 等待就绪超时时间
- `APP_EFFECTIVE_VERSION` (optional): 若提供，则校验 `/api/health` 返回的 `version` 与其一致

### Behavior

- 启动 `catnap` 并等待就绪
- 必须通过（最小集合）：
  - `GET /api/health` → `200` 且 `status=ok`
  - `GET /` → `200` 且返回 HTML（可用 `<!doctype html>` 作为最小断言）
  - （可选）若提供 `APP_EFFECTIVE_VERSION`：`/api/health.version` 必须与其一致
- 结束时必须清理子进程（即使失败也要退出前 kill）

### Exit codes

- `0`: smoke test 通过
- `!= 0`: smoke test 失败（启动失败/超时/端点不符合契约）

## `.github/scripts/release-intent.sh` (planned)

- Scope: internal
- Change: New

### Purpose

在 CI 中将“合并到 main 的 commit”映射为发布意图（`should_release` + `bump_level`），以便 gate 自动发版链路。

### Inputs

- `GITHUB_TOKEN`（required）：用于查询 GitHub API（读取 commit 关联 PR 与 labels）
- `GITHUB_REPOSITORY`（required）：`<owner>/<repo>`
- `GITHUB_SHA`（required）：需要判定的 commit SHA（通常为 `push main` 的 SHA）
- Label contract（required）：见 `contracts/file-formats.md` 的 `type:*` 集合与互斥规则

### Outputs

- 向 `$GITHUB_OUTPUT` 写入：
  - `should_release=true|false`
  - `bump_level=major|minor|patch|none`

### Exit codes

- `0`: 判定成功
- `!= 0`: 判定失败（例如无法关联到 PR、或标签不符合契约；失败策略由 workflow 冻结）

### Behavior (normative)

- 读取 commit 关联 PR 的 labels，并按 `contracts/file-formats.md` 的 “Label mapping” 输出：
  - `type:major` → `should_release=true`, `bump_level=major`
  - `type:minor` → `should_release=true`, `bump_level=minor`
  - `type:patch` → `should_release=true`, `bump_level=patch`
  - `type:docs` / `type:skip` → `should_release=false`, `bump_level=none`
- 当 commit 无法关联 PR 时：
  - 必须按 `contracts/file-formats.md` 的 “No PR / direct push policy” 冻结策略执行（跳过或允许；不得默默随机行为）

## `.github/scripts/label-gate.sh` (planned)

- Scope: internal
- Change: New

### Purpose

在 PR 阶段强制“意图标签互斥且必须 1 个”，并产出用于后续 jobs 的统一信号。

### Inputs

- `GITHUB_TOKEN`（required）
- `GITHUB_REPOSITORY`（required）
- `PR_NUMBER`（required）
- Allowed labels（required）：见 `contracts/file-formats.md` 的标签集合

### Outputs

- 向 `$GITHUB_OUTPUT` 写入：
  - `intent_label=type:docs|type:skip|type:patch|type:minor|type:major`
  - `should_release=true|false`
  - `bump_level=major|minor|patch|none`

### Exit codes

- `0`: 校验通过
- `!= 0`: 校验失败（缺少意图标签 / 同时存在多个意图标签 / 存在未知意图标签）
