# CI/CD：补齐漏发版本并修复 squash merge 自动发版识别（z7myy）

## 状态

- Status: 待实现
- Created: 2026-03-09
- Last: 2026-03-09

## 背景 / 问题陈述

当前仓库使用 `push main` + `release-intent` gate 驱动自动发版。该策略默认保守：

- 先通过 GitHub `commits/{sha}/pulls` API 解析 merge 后提交对应的 PR
- 成功解析到且仅解析到一个 PR 时，再读取其 `type:*` 标签决定是否发版
- 若无法判定，则继续跑 CI，但跳过自动发版

`PR #60` 合并到 `main` 后，主分支 CI run `22835986841` 中的 `Release (tag/assets/image)` 被跳过；合并提交 `cafc2179b10fa846d9ac0302d1c129618be7e13b` 未能通过 API 反查到关联 PR，且现有 fallback 仅支持 `Merge pull request #<n>` 形式的 merge commit 标题，不支持 squash merge 常见的 `... (#<n>)` 尾缀。

结果是：

- latest stable release 仍停留在 `v0.5.4`
- 预期由 `type:minor` 驱动产生的 `v0.6.0` 尚未发布

本规格用于同时完成两件事：补齐这次漏掉的 stable release，并修复后续 squash merge 的自动发版识别缺口。

## 目标 / 非目标

### Goals

- 对保守 gate 做最小增强：当 `commits/{sha}/pulls` 返回空集合时，允许对 commit subject 尾缀 ` (#<pr>)` 启用 squash fallback。
- 保持现有安全语义：若无法可靠判定 PR，仍继续跳过自动发版。
- 为 `release-intent.sh` 增加本地可重复、无需实时 GitHub API 的脚本级回归测试。
- 使用现有 `CI Pipeline` 的 `workflow_dispatch ref=main + bump_level=minor` 补发当前缺失版本。
- 更新 README 发布说明，使维护者知道 squash fallback 与 manual publish 的兜底口径。

### Non-goals

- 不迁移到新的 release workflow 架构（例如独立 `workflow_run` 链路或额外 reconciliation 服务）。
- 不修改 semver 规则、release assets 矩阵、GHCR 命名策略或 `latest` 更新规则。
- 不改动运行时 HTTP API、UI 行为或其他非 CI/CD 功能。

## 范围（Scope）

### In scope

- `.github/scripts/release-intent.sh`：增加严格的 squash merge subject fallback。
- `.github/scripts/test-release-intent.sh` 与本地 fixtures：覆盖 API 主路径、merge fallback、squash fallback、manual publish、无 PR skip、无效标签 skip。
- `.github/workflows/ci.yml`：接入上述脚本测试，使 CI 能稳定回归。
- `README.md`：补充 squash fallback 与 manual publish/backfill 口径。
- 现网补发：以 `cafc2179b10fa846d9ac0302d1c129618be7e13b` 为基线，用 `workflow_dispatch ref=main + bump_level=minor` 补发 `v0.6.0`。

### Out of scope

- 更宽松的 subject 解析（例如任意位置提取 `#123`、解析多个 PR 号、基于正文/描述推断 PR）。
- 引入新的测试框架（Bats、ShellSpec 等）或新的发布工具链。

## 需求（Requirements）

### MUST

- PR 解析顺序
  - 优先保留 GitHub `commits/{sha}/pulls` API 作为主路径。
  - 仅当主路径返回 0 个 PR 时，才允许尝试 subject fallback。
- subject fallback 规则
  - merge commit 继续支持 `Merge pull request #<n>`。
  - squash commit 仅接受 subject 尾缀严格匹配 ` (#<digits>)`。
  - 若 subject 不匹配、PR 不存在、获取 labels 失败、或 label 不合法，则 `should_release=false`。
- 测试覆盖
  - 回归测试必须本地可重复，且不依赖实时 GitHub API。
  - 至少覆盖：API 命中、merge fallback、squash fallback、manual publish、无 PR 继续 skip、无效标签 skip。
- manual publish/backfill
  - 当前缺失版本必须通过现有 `CI Pipeline` 的 `workflow_dispatch ref=main + bump_level=minor` 补齐。
  - 不使用 `release-backfill.yml` 补这次缺失版本，因为该 workflow 只接受已存在 tag。
- 文档同步
  - README 需要明确：squash merge 的 `(#PR)` 尾缀可作为保守 fallback；若仍无法判定 PR，则继续跳过自动发版。

## 验收标准（Acceptance Criteria）

- Given `commits/{sha}/pulls` 成功返回唯一 PR，When 执行 `release-intent.sh`，Then 行为与当前实现一致。
- Given API 返回空集合，When commit subject 为 `feat: ... (#60)` 且 `#60` 标签为 `type:minor`，Then 输出 `should_release=true`、`bump_level=minor`、`pr_number=60`。
- Given API 返回空集合，When commit subject 不匹配尾缀 ` (#<digits>)`，Then 输出 `should_release=false` 且跳过自动发版。
- Given fallback 解析到 PR 但其 `type:*` 标签无效，When 执行脚本，Then 输出 `should_release=false` 且不得误发版。
- Given 当前 stable release 为 `v0.5.4`，When 对 `main` 手动触发 `workflow_dispatch bump_level=minor`，Then 发布 `v0.6.0`，并生成 release assets 与 GHCR `v0.6.0` / `latest`。
- Given 修复 PR 使用 `type:skip`，When 合并到 `main`，Then 只运行 lint/tests，不产生新的 stable release。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `bash ./.github/scripts/test-release-intent.sh`
- 现有 CI checks 全部通过
- manual publish run 成功并可对账 release assets / GHCR tag

### Quality checks

- 不引入新的测试框架或发布工具。
- 不放宽“无法判定即 skip”的保守默认。

## 文档更新（Docs to Update）

- `README.md`：更新 release-intent fallback 与 manual publish/backfill 说明。
- `docs/specs/README.md`：登记本规格并跟踪收敛状态。

## 实现里程碑（Milestones / Delivery checklist）

- [ ] M1: 新建规格并固化基线证据（`v0.5.4` latest、`v0.6.0` 不存在、run `22835986841` skip release）。
- [ ] M2: 实现 `release-intent.sh` 的 squash fallback，保持主路径优先与保守默认。
- [ ] M3: 接入脚本级回归测试与 README 文档同步。
- [ ] M4: 触发 `workflow_dispatch ref=main + bump_level=minor` 补齐 `v0.6.0` 并完成对账。
- [ ] M5: 创建并合并 `type:skip` 修复 PR，完成 checks/review 收敛与最终验证。

## 方案概述（Approach, high-level）

- 先补文档规格并冻结验收。
- 用最小脚本改动补上 squash fallback，同时为脚本建立 stub-based regression harness。
- 在 PR 合并前先补发当前缺失版本，避免继续处于“main 已包含功能但 release 缺失”的不一致状态。
- 最后用 `type:skip` 的 CI-only 修复 PR 收敛主分支行为，确保后续 squash merge 不再漏判。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：若补发前 `main` 或 stable tag 已变化，必须先重算目标版本，不能盲目假定仍为 `v0.6.0`。
- 风险：subject fallback 只能覆盖形如 `... (#123)` 的 squash merge；其他异常标题仍会被保守跳过。
- 开放问题：无。
- 假设：当前 `main` 仍停在 `cafc2179b10fa846d9ac0302d1c129618be7e13b`，且 `v0.6.0` 尚不存在。

## 变更记录（Change log）

- 2026-03-09: 新建规格，冻结漏发版本补齐与 squash fallback 修复目标。
