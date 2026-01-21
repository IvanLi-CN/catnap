# CI/CD：自动发版意图标签与版本号策略（防止 docs-only 发版）（0005）

## 状态

- Status: 已完成
- Created: 2026-01-21
- Last: 2026-01-21

## 背景 / 问题陈述

在当前工作流中，PR 通常分为两类：

- 设计/文档更新（不应触发发版）
- 程序实现更新（应触发发版）

现状是：合并到 `main` 后会进入自动发布链路（tag / GitHub Release / GHCR），且版本号由 CI 计算。
这会导致仅包含文档/设计类变更的合并也产生新版本迭代，造成“无产物变化”的版本噪音。

本计划补齐一套**可审计、可强制、可自动化**的“发版意图（release intent）”规范：通过 PR 标签明确表达“是否发版 + bump 等级”，并在 CI 中强制执行。

> 注：本计划只解决“自动发版触发门槛 + 版本号策略”。完整发布链路（release assets / GHCR multi-arch / smoke test / UI embed 等）已在计划 0003 中落地。

## 目标 / 非目标

### Goals

- 合并到 `main` 的提交必须可判定“是否允许自动发版”，且该判定对维护者可见、对 CI 可强制。
- 仅文档/设计类变更不得触发自动发版（不得创建新 tag / Release，不得推送新的 `v<semver>` 镜像 tag）。
- 程序实现类变更通过标签显式声明 bump 等级（`major|minor|patch`），CI 负责计算并发布新版本。
- 无法关联到 PR 的 `push main`（direct push / 异常合并）默认跳过自动发版（只允许 `workflow_dispatch` 手动发版）。

### Non-goals

- 不重新定义 release assets 矩阵、GHCR 镜像命名、smoke test 与 UI embed（沿用 0003 现有契约与实现）。
- 不引入新的版本管理工具链（changesets / semantic-release 等）。
- 不要求 PR 显式改 `Cargo.toml` 版本号（由 CI 计算有效版本号）。

## 用户与场景

- 维护者：合并 PR 时明确选择“是否发版”，并通过 CI 强制执行，避免误发版或漏发版。
- 部署者：只在真正发布的变更上看到新版本，版本序列更干净。
- CI：在 PR 阶段即可发现缺失/冲突标签，减少合并后才发现发布不符合预期。

## 范围（Scope）

### In scope

- PR 标签契约（互斥且必须 1 个）与 CI enforce：
  - `type:docs` / `type:skip` / `type:patch` / `type:minor` / `type:major`
- PR 阶段新增 `label-gate`：缺失/冲突/未知标签必须失败并给出清晰提示。
- `push main` 阶段新增 `release-intent`：将 merge commit 映射为 `should_release` 与 `bump_level`：
  - 无关联 PR：`should_release=false`
  - `type:docs|type:skip`：`should_release=false`
  - `type:patch|minor|major`：`should_release=true` + 对应 `bump_level`
- 版本号策略：
  - base version 取仓库现存 `v<semver>` tags 的语义版本最大值；无 tag fallback `Cargo.toml` 的 `version`
  - bump math：按 `major|minor|patch`
  - 唯一性：若目标 tag 已存在，继续递增 patch 直到未占用
- `workflow_dispatch` 手动发版语义补齐（ref=`main` 必须显式提供 `bump_level`；ref=tag 仅重跑该版本，不更新 `latest`）。

### Out of scope

- 处理“一个 merge commit 关联多个 PR”这种非典型情况的复杂仲裁逻辑（如确需支持，在本计划外单独冻结规则）。

## 需求（Requirements）

### MUST

- 标签契约
  - PR 必须且只能包含一个意图标签（互斥且必须 1 个）。
  - 缺失/冲突/未知标签必须在 PR 阶段被 CI 拦截并失败。
- 自动发版门槛
  - `type:docs` 与 `type:skip` 合并到 `main` 后不得触发自动发版（tag / GitHub Release / GHCR）。
  - `type:patch|minor|major` 合并到 `main` 后允许自动发版，并按标签计算新版本号。
  - 无关联 PR 的 `push main` 必须跳过自动发版（可继续跑 lint/tests）。
- 版本号策略（CI 计算）
  - base version：语义版本最大 tag（无 tag fallback `Cargo.toml`）。
  - bump：`major` → `(X+1).0.0`，`minor` → `X.(Y+1).0`，`patch` → `X.Y.(Z+1)`。
  - tag format 固定为 `v<semver>`。
  - 唯一性：若目标 tag 已存在，继续递增 patch 直到未占用版本。
- 可观测与可排障
  - CI 日志必须输出：识别到的 PR、意图标签、`should_release`、`bump_level`、base version、目标版本与目标 tag。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| PR label set & semantics | File format | external | New | ./contracts/file-formats.md | maintainer | contributors/maintainers | 互斥且必须 1 个 |
| `.github/scripts/label-gate.sh` (planned) | CLI | internal | New | ./contracts/cli.md | maintainer | CI | PR 阶段强制标签 |
| `.github/scripts/release-intent.sh` (planned) | CLI | internal | New | ./contracts/cli.md | maintainer | CI | push main 映射意图 |
| `.github/scripts/compute-version.sh` | CLI | internal | Modify | ./contracts/cli.md | maintainer | CI | 基于 bump 计算版本 |
| Git tag naming | File format | external | Modify | ./contracts/file-formats.md | maintainer | users/deployers | `v<semver>` |

### 契约文档（按 Kind 拆分）

- [contracts/README.md](./contracts/README.md)
- [contracts/cli.md](./contracts/cli.md)
- [contracts/file-formats.md](./contracts/file-formats.md)

## 约束与风险（Constraints & Risks）

- 依赖 GitHub API 获取 commit 关联 PR 与 labels：需要明确 token 权限（只读即可）与错误处理策略（超时/找不到 PR 的默认行为必须可预测）。
- 标签策略改变了维护流程：需要在仓库 README/贡献指南中给出最小提示，避免新贡献者卡在 CI gate。

## 验收标准（Acceptance Criteria）

- Given 一个 PR 指向 `main`
  When PR 缺少任何 `type:*` 意图标签
  Then PR 的 CI 必须失败，并提示必须选择且只能选择一个：`type:docs|skip|patch|minor|major`

- Given 一个 PR 指向 `main`
  When PR 同时存在 `type:docs` 与 `type:patch`（或任意多个意图标签）
  Then PR 的 CI 必须失败，并提示“意图标签互斥”

- Given 合并到 `main` 的 PR 标签为 `type:docs` 或 `type:skip`
  When CI 在 `main` 上运行
  Then 不得创建新的 git tag / GitHub Release，且不得推送新的 `v<semver>` 镜像 tag

- Given base version 为 `v0.1.4`
  And 合并到 `main` 的 PR 标签为 `type:minor`
  When CI 计算 `APP_EFFECTIVE_VERSION`
  Then 目标版本必须为 `0.2.0`（若 `v0.2.0` 已存在则继续递增 patch 直到未占用）

- Given `push main` 的提交无法关联到任何 PR
  When CI 进入发布 gating 阶段
  Then `should_release=false` 并跳过自动发版（允许继续跑 lint/tests）

## 实现前置条件（Definition of Ready / Preconditions）

- 已冻结标签集合与语义：`type:docs|skip|patch|minor|major`
- 已冻结无 PR 策略：`should_release=false`
- 已冻结 base version：语义版本最大 tag（无 tag fallback `Cargo.toml`）

## 非功能性验收 / 质量门槛（Quality Gates）

- 不新增新的 lint/test 工具；沿用现有 CI 门槛。
- 失败提示必须可读、可操作（让贡献者知道“该打哪个标签、在哪里打”）。

## 文档更新（Docs to Update）

- `README.md`：补充“PR 必须选择 `type:*` 意图标签”的最小说明与示例

## 实现里程碑（Milestones）

- [x] M1: PR label gate：新增/接入 `label-gate`，缺失/冲突/未知标签直接失败
- [x] M2: Release intent：新增/接入 `release-intent`，实现 commit→PR→labels 映射与无 PR 跳过
- [x] M3: Versioning：升级 `compute-version.sh` 支持 `BUMP_LEVEL` + base=highest tag + bump math + uniqueness
- [x] M4: Wiring：`push main` 发布 jobs 仅在 `should_release=true` 时执行
- [x] M5: 文档同步：`README.md` 补齐标签规则与手动发版参数说明

## 开放问题（需要主人回答）

None.

## 假设（Assumptions，待主人确认）

None.

## 变更记录（Change log）

- 2026-01-21: 创建计划 0005，冻结 PR 标签意图、无 PR 跳过策略、base version 与 bump 规则。
- 2026-01-21: 实现：PR `label-gate` + `release-intent` + `compute-version`（bump/base/uniqueness）+ release job gating；同步 README。
