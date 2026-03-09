# CI/CD：补齐漏发版本并修复 squash merge 自动发版识别（z7myy）

## 状态

- Status: 部分完成（5/6）
- Created: 2026-03-09
- Last: 2026-03-09

## 背景 / 问题陈述

仓库使用 `push main` + `release-intent` gate 驱动自动发版，默认策略比较保守：

- 先通过 GitHub `commits/{sha}/pulls` API 解析 merge 后提交对应的 PR
- 成功解析到且仅解析到一个 PR 时，再读取其 `type:*` 标签决定是否发版
- 若无法可靠判定，则继续跑 CI，但跳过自动发版

`PR #60` 合并到 `main` 后，主分支 CI run `22835986841` 中的 `Release (tag/assets/image)` 被跳过；合并提交 `cafc2179b10fa846d9ac0302d1c129618be7e13b` 没能通过 API 反查到关联 PR，而旧 fallback 仅支持 `Merge pull request #<n>`，无法识别 squash merge 常见的 `... (#<n>)` 尾缀。

这带来了两个需要一起闭环的问题：

- latest stable release 仍停留在 `v0.5.4`，预期由 `type:minor` 驱动的 `v0.6.0` 未发布
- manual publish 的 release job 在 GHCR multi-arch push 阶段重复进入 Dockerfile 内的 Rust 构建，导致发布闭环很慢，旧 tag 的 backfill 也缺少统一的轻量封装路径

## 目标 / 非目标

### Goals

- 对保守 gate 做最小增强：当 `commits/{sha}/pulls` 返回空集合时，允许对 commit subject 尾缀 ` (#<pr>)` 启用 squash fallback。
- 保持现有安全语义：若无法可靠判定 PR，仍继续跳过自动发版。
- 为 `release-intent.sh` 增加本地可重复、无需实时 GitHub API 的脚本级回归测试。
- 使用现有 `CI Pipeline` 的 `workflow_dispatch ref=main + bump_level=minor` 补齐缺失的 `v0.6.0`。
- 让 release / backfill workflow 在推送 GHCR 镜像时复用已产出的 linux gnu binaries，避免在 Docker multi-arch push 阶段重复编译 Rust。
- 更新 README 发布说明，使维护者知道 squash fallback、manual publish / backfill 口径，以及新的镜像封装路径。

### Non-goals

- 不迁移到新的 release workflow 架构（例如独立 `workflow_run` 链路或额外 reconciliation 服务）。
- 不修改 semver 规则、release assets 矩阵、GHCR 命名策略或 `latest` 更新规则。
- 不改动运行时 HTTP API、UI 行为或其他非 CI/CD 功能。

## 范围（Scope）

### In scope

- `.github/scripts/release-intent.sh`：增加严格的 squash merge subject fallback。
- `.github/scripts/test-release-intent.sh` 与本地 fixtures：覆盖 API 主路径、merge fallback、squash fallback、manual publish、无 PR skip、无效标签 skip。
- `.github/scripts/prepare-docker-binaries.sh` 与 `.github/scripts/test-prepare-docker-binaries.sh`：把已产出的 linux gnu binaries 组织成 `dist/docker/**` 发布上下文，并生成专用 `Dockerfile.release`。
- `.github/workflows/ci.yml`：接入上述脚本测试；在 PR release smoke 中验证基于预构建 binary 的镜像封装；在正式 release job 中使用 `dist/docker/**` 做 GHCR 封装。
- `.github/workflows/release-backfill.yml`：复用主分支上的发布辅助脚本，让已存在 tag 的 backfill 也走同一镜像封装路径。
- `README.md`：补充 squash fallback、manual publish / backfill 与 GHCR 镜像封装说明。
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
  - 预构建二进制封装脚本必须覆盖：默认双架构、单架构 smoke、自定义源路径、缺失 binary 失败、非法 arch 失败。
- manual publish / backfill
  - 当前缺失版本必须通过现有 `CI Pipeline` 的 `workflow_dispatch ref=main + bump_level=minor` 补齐。
  - 当 `CI Pipeline` 已创建 tag 但尚未成功产出 release / GHCR 时，允许在主分支修复发布链路后再用 `release-backfill.yml` 对该 tag 做补齐。
- release packaging
  - Release / backfill workflow 在进入 GHCR multi-arch push 前，必须复用已成功产出的 linux gnu release binaries，不得在 Dockerfile 内再次执行 `cargo build --release`。
  - PR 阶段必须有一个可重复的 smoke 验证，确保 `dist/docker/Dockerfile.release` 能基于预构建 binary 成功封装 `linux/amd64` 镜像。
- 文档同步
  - README 需要明确：squash merge 的 `(#PR)` 尾缀可作为保守 fallback；若仍无法判定 PR，则继续跳过自动发版。
  - README 需要说明 GHCR 镜像封装会复用已产出的 linux gnu binaries。

## 验收标准（Acceptance Criteria）

- Given `commits/{sha}/pulls` 成功返回唯一 PR，When 执行 `release-intent.sh`，Then 行为与旧实现一致。
- Given API 返回空集合，When commit subject 为 `feat: ... (#60)` 且 `#60` 标签为 `type:minor`，Then 输出 `should_release=true`、`bump_level=minor`、`pr_number=60`。
- Given API 返回空集合，When commit subject 不匹配尾缀 ` (#<digits>)`，Then 输出 `should_release=false` 且跳过自动发版。
- Given fallback 解析到 PR 但其 `type:*` 标签无效，When 执行脚本，Then 输出 `should_release=false` 且不得误发版。
- Given release assets 已成功产出，When workflow 进入 GHCR 镜像发布阶段，Then 镜像封装必须基于 `dist/docker/**` 中的预构建 linux gnu binaries 完成，而不是在 Dockerfile 内重新执行 Rust 构建。
- Given 当前 stable release 为 `v0.5.4`，When 对 `main` 手动触发 `workflow_dispatch bump_level=minor`，Then 发布 `v0.6.0`，并生成 release assets 与 GHCR `v0.6.0` / `latest`。
- Given tag 已存在但 release / GHCR 不完整，When 触发 `release-backfill.yml`，Then 必须沿用相同的 `dist/docker/**` 封装路径完成补齐。
- Given 修复 PR 使用 `type:skip`，When 合并到 `main`，Then 只运行 lint/tests，不产生新的 stable release。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- `bash ./.github/scripts/test-release-intent.sh`
- `bash ./.github/scripts/test-prepare-docker-binaries.sh`
- `/bin/bash -n .github/scripts/release-intent.sh`
- `/bin/bash -n .github/scripts/prepare-docker-binaries.sh`
- `/bin/bash -n .github/scripts/test-prepare-docker-binaries.sh`
- `ruby -e 'require "yaml"; YAML.load_file(".github/workflows/ci.yml"); YAML.load_file(".github/workflows/release-backfill.yml")'`
- 现有 CI checks 全部通过
- manual publish 或 release-backfill run 成功并可对账 release assets / GHCR tag

### Quality checks

- 不引入新的测试框架或发布工具。
- 不放宽“无法判定即 skip”的保守默认。

## 文档更新（Docs to Update）

- `README.md`：更新 release-intent fallback、manual publish / backfill 与 GHCR 镜像封装说明。
- `docs/specs/README.md`：登记本规格并跟踪收敛状态。

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 新建规格并固化基线证据（`v0.5.4` latest、`v0.6.0` 不存在、run `22835986841` skip release）。
- [x] M2: 实现 `release-intent.sh` 的 squash fallback，保持主路径优先与保守默认。
- [x] M3: 接入脚本级回归测试与 README 文档同步。
- [x] M4: 让 release / backfill workflow 复用预构建 binaries 完成 GHCR 镜像封装，并补齐 PR smoke 验证。
- [x] M5: 触发 `workflow_dispatch ref=main + bump_level=minor` 补齐 `v0.6.0` 并完成对账。
- [ ] M6: 创建并收敛 `type:skip` 修复 PR，确保后续 squash merge 不再漏判且不会额外发 stable release。

## 方案概述（Approach, high-level）

- 先补文档规格并冻结验收。
- 用最小脚本改动补上 squash fallback，同时为脚本建立 stub-based regression harness。
- 将 release / backfill 的 GHCR 封装改为复用现成的 linux gnu binaries，并在 PR 阶段提前 smoke 验证这个发布上下文。
- 用 `workflow_dispatch ref=main + bump_level=minor` 补齐当前缺失版本，避免继续处于“main 已包含功能但 release 缺失”的不一致状态。
- 最后用 `type:skip` 的 CI-only 修复 PR 收敛主分支行为，确保后续 squash merge 不再漏判。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：subject fallback 只能覆盖形如 `... (#123)` 的 squash merge；其他异常标题仍会被保守跳过。
- 风险：发布 workflow 现在依赖先成功产出 linux gnu release binaries；若前置 release asset 构建失败，后续 GHCR 封装也必须立即失败，不能静默回退为旧路径。
- 开放问题：无。
- 假设：GitHub `commits/{sha}/pulls` 对 squash merge 仍可能返回空集合，因此保守 fallback 仍有必要保留。

## 验证证据（Validation evidence）

- 自动发版修复：PR `#62`（`type:skip`）包含 `.github/scripts/release-intent.sh` 的 squash fallback、fixture / stub 回归测试，以及 CI 中的脚本回归接线。
- 补发 run：`CI Pipeline` run `22850192479`（`workflow_dispatch`, `ref=main`, `bump_level=minor`）已在 2026-03-09 12:41 UTC 完成 `success`。
- 发布结果：GitHub Release `v0.6.0` 已于 2026-03-09 12:41 UTC 发布，latest stable release 也已更新到 `v0.6.0`。
- Release assets：`v0.6.0` 附带 8 个 assets（linux amd64/arm64 × gnu/musl + 对应 `.sha256`）。
- Docker publish gate：run `22850192479` 的 job `Release (tag/assets/image)` 中，`Build and push Docker image`、`Verify docker push gate` 与 `Create/Update GitHub Release and upload assets` 均为 `success`。
- 本地回归：`bash ./.github/scripts/test-release-intent.sh`、`bash ./.github/scripts/test-prepare-docker-binaries.sh`、`/bin/bash -n .github/scripts/{release-intent,prepare-docker-binaries,test-prepare-docker-binaries}.sh`、`ruby -e 'require "yaml"; YAML.load_file(".github/workflows/ci.yml"); YAML.load_file(".github/workflows/release-backfill.yml")'`。

## 变更记录（Change log）

- 2026-03-09: 新建规格，冻结漏发版本补齐与 squash fallback 修复目标。
- 2026-03-09: 完成 `release-intent` squash fallback、stub 回归测试与 README 同步。
- 2026-03-09: 为 release / backfill job 增加预构建二进制封装链路与回归测试，避免 GHCR 推送阶段重复编译 Rust。
- 2026-03-09: `CI Pipeline` run `22850192479` 成功补发 `v0.6.0`；GitHub Release 已发布 8 个 assets，latest stable 已更新到 `v0.6.0`。
