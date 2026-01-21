# CI/CD：修复 Release job 失败（#0007）

## 状态

- Status: 待实现
- Created: 2026-01-21
- Last: 2026-01-21

## 背景 / 问题陈述

- 当前 `.github/workflows/ci.yml` 的 `Release (tag/assets/image)` job 在 `push main` 路径会失败。
- baseline 观测：
  - Actions run `21201355431`（`push`，2026-01-21）
  - 失败位置：`Determine release version/tag`
  - 失败原因（核心事实）：该 step 在同一个 shell 中 `set -euo pipefail`，并尝试读取 `APP_EFFECTIVE_VERSION`，但它来自于 `bash ./.github/scripts/compute-version.sh` 的子进程，未在当前 shell 中定义，从而触发 `unbound variable` 并退出。

## 目标 / 非目标

### Goals

- 使 `Release (tag/assets/image)` job 在 `push main` 与 `push tag` 两条路径都能稳定通过“版本/标签决策”步骤（不再出现 `unbound variable`）。
- 明确并冻结“版本号获取”的内部契约：`compute-version.sh` 必须可在同 step 内被调用并返回 machine-readable 的版本号（不依赖外部解析日志文本）。
- 提供可验证的验收标准：通过 GitHub Actions run 的日志即可验证修复有效。

### Non-goals

- 不在本计划中引入/实现“发版意图标签 gate”（该口径由 Plan #0005 承担）。
- 不在本计划中做“性能提速”（缓存/runner/并行等）；只修复 correctness。
- 不改变版本号策略的业务口径（bump 规则/标签策略等），仅修复“值如何在 step 内被读取”。

## 范围（Scope）

### In scope

- 修改 `.github/workflows/ci.yml` 的 `Determine release version/tag` step，使其在 `set -u` 下也能正确读取版本号。
- 必要时修改 `.github/scripts/compute-version.sh`（或新增小包装脚本）以提供 machine-readable 输出能力。

### Out of scope

- Release 完整链路的提速与 runner 策略（另见 Plan #0006）。
- 任意与 Release intent labels / gating 相关的行为变更（另见 Plan #0005）。

## 需求（Requirements）

### MUST

- `Determine release version/tag` step 必须在单 step 内获得 `version`（与 `tag`）的确定值，不依赖“写入 `$GITHUB_ENV` 后再读回”（因为 `$GITHUB_ENV` 只对后续 steps 生效）。
- `compute-version.sh`（或等价实现）必须提供 machine-readable 的输出模式（见契约），以便调用方安全读取版本号。
- 修复后在 `push main`（无 tag）路径：
  - step 必须能计算 `version` 与 `tag` 并写入 `GITHUB_OUTPUT`；
  - step 内不得出现 `unbound variable`。
- 修复后在 `push tag` 路径：
  - step 必须基于 tag 解析 `version`，并写入 outputs；
  - 若 tag 不符合约定（例如不以 `v` 开头），必须明确失败并输出可诊断错误。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `.github/scripts/compute-version.sh` (machine-readable) | CLI | internal | Modify | ./contracts/cli.md | maintainer | CI | 允许同 step 读取 version |

### 契约文档（按 Kind 拆分）

- [contracts/README.md](./contracts/README.md)
- [contracts/cli.md](./contracts/cli.md)

## 验收标准（Acceptance Criteria）

- Given `push` 到 `main` 且 `HEAD` 上不存在 `v<semver>` tag
  When `Release (tag/assets/image)` job 运行到 `Determine release version/tag`
  Then step 必须成功结束，并在 outputs 中写入：
  - `tag`（以 `v` 开头）
  - `version`（semver）
  - `is_main=true`

- Given `push` 到 `main` 且 `HEAD` 上存在 `v<semver>` tag
  When `Determine release version/tag` 运行
  Then step 必须复用该 tag，并输出一致的 `version`，且不触发 `compute-version.sh` 的 bump 逻辑

- Given `push` 一个 tag（`refs/tags/v<semver>`）
  When `Determine release version/tag` 运行
  Then step 必须解析 `version=<semver>`，并输出 `is_main=false`

- Given `push` 一个不合法 tag（例如不以 `v` 开头）
  When `Determine release version/tag` 运行
  Then step 必须明确失败，并输出可诊断的错误信息

## 实现前置条件（Definition of Ready / Preconditions）

- None（本计划为 correctness bugfix，口径与验收已冻结）

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- 依赖 CI 真实运行验证；无需引入新测试框架或工具。

### Quality checks

- 不引入新 lint/typecheck 工具；保持现有 CI 门槛不变。

## 文档更新（Docs to Update）

- `README.md`: 若 README 中已有 Release/Manual publish 说明，需要同步“tag/version 生成口径”与排障入口（只做最小必要更新）。

## 实现里程碑（Milestones）

- [ ] M1: 为 `compute-version.sh` 增加 machine-readable 输出模式（契约先行）
- [ ] M2: 修复 `Determine release version/tag` step，确保同 step 内可读取版本号（不依赖 `$GITHUB_ENV`）
- [ ] M3: 通过一次 `push main` 与一次 `push tag` 的 CI run 验证修复（日志可证）

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：`actions/checkout` 未拉取 tags 时，`git tag --points-at HEAD` 可能观测不到既有 tags；实现阶段需要明确是否启用 `fetch-tags: true`（作为 bugfix 的一部分）。
- 开放问题：None
- 假设：None

## 变更记录（Change log）

- 2026-01-21: 新建计划，冻结 Release job 失败的根因与修复契约。

## 参考（References）

- baseline push run（Release 失败）：`https://github.com/IvanLi-CN/catnap/actions/runs/21201355431`
