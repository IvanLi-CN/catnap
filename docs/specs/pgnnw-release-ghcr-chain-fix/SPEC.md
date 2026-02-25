# 发布链路修复与 GHCR 回填闭环（Dockrev 无候选）（#pgnnw）

## 状态

- Status: 部分完成（4/5）
- Created: 2026-02-25
- Last: 2026-02-25

## 背景 / 问题陈述

- `home-lab-catnap` 线上服务当前镜像解析为 `ghcr.io/ivanli-cn/catnap:v0.1.9`，Dockrev 无候选更新。
- `v0.2.2` GitHub Release 已存在，但 GHCR 缺失对应 tag/manifest，导致 Dockrev 基于 digest 比较无法产出候选。
- 关键 run（`22259249496` 及多次 backfill）在 Docker push 阶段被取消，且 release 先于镜像推送执行，形成“release 存在但镜像缺失”的断层。

## 目标 / 非目标

### Goals

- 修复 `release -> GHCR` 发布链路，确保 stable release 对应镜像 tag 始终存在。
- 将 `latest` 策略固定为“跟随仓库最高 stable semver tag（含 backfill）”。
- 回填缺失镜像 `v0.2.0`、`v0.2.1`、`v0.2.2`，并输出可追溯验证证据。

### Non-goals

- 不修改 Dockrev 候选算法语义。
- 不改动线上 compose 架构与业务代码。

## 范围（Scope）

### In scope

- `.github/workflows/ci.yml` 发布顺序、push 硬门禁、latest 判定与 timeout 调整。
- `.github/workflows/release-backfill.yml` 与主链路行为对齐。
- `README.md` 发布口径与 manual/backfill 策略更新。
- 合并后执行 backfill 与验证闭环。

### Out of scope

- Dockrev 产品逻辑调整。
- 跨仓库重构或部署拓扑调整。

## 需求（Requirements）

### MUST

- Docker push 未成功时，workflow 明确失败，release 不得发布。
- `latest` 仅在“当前发布 tag 等于仓库最高 stable tag”时更新。
- `release` 与 `release-backfill` 统一 180 分钟 timeout，降低 90 分钟边界取消风险。
- 回填 `v0.2.0~v0.2.2` 后，GHCR 三个 tag 可查询，且 `latest` 与 `v0.2.2` 对齐。

### SHOULD

- 在 workflow 中输出关键判定信息（primary/fallback outcome、highest stable tag、publish latest 布尔值），便于排障。

### COULD

- 后续在 Dockrev 增加“release 存在但 registry 缺失”提示文案（本规格不实现）。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- 发布链路先完成 Docker push（含 fallback），再执行 GitHub Release publish。
- push 成功判定规则：`primary=success`，或 `primary=failure 且 fallback=success`；其余状态全部失败。
- `latest` 判定通过 git tag 计算最高 stable semver，并与当前发布 tag 对比。

### Edge cases / errors

- 若仓库 stable tag 解析为空，workflow 直接失败，防止误推 latest。
- 若 primary 被 cancelled/skipped，fallback 不触发成功路径，直接门禁失败。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `.github/workflows/ci.yml` release job | file/workflow | internal | Modify | None | maintainer | release pipeline | 调整顺序与门禁 |
| `.github/workflows/release-backfill.yml` | file/workflow | internal | Modify | None | maintainer | backfill pipeline | 与主链路对齐 |
| `README.md` 发布说明 | docs | external | Modify | None | maintainer | operators | latest/backfill 口径统一 |

### 契约文档（按 Kind 拆分）

- None

## 验收标准（Acceptance Criteria）

- Given `v0.2.2` release 存在，When 发布/回填完成，Then `ghcr.io/ivanli-cn/catnap:v0.2.2` manifest 可解析。
- Given 执行 `v0.2.0~v0.2.2` 回填，When 查询 GHCR，Then 三个 tag 都存在，且 `latest` digest 与 `v0.2.2` digest 一致。
- Given Docker push 未成功，When workflow 到达门禁步骤，Then job 失败且 release 步骤不执行。
- Given 触发 Dockrev runtime scan，When 查看目标服务，Then 候选状态与 registry 事实一致并可解释。

## 实现前置条件（Definition of Ready / Preconditions）

- 发布策略与 latest 策略已冻结。
- 回填范围已确认为 `v0.2.0~v0.2.2`。
- 允许 fast-track 自动推进 push/PR/checks/review-loop/merge。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- 非破坏性校验：workflow 条件分支与门禁路径静态校验。
- 远端验证：PR checks、backfill run 结果、GHCR tag 查询、线上 pull 与 Dockrev scan 对账。

### Quality checks

- 保持现有 CI 门禁，不引入新工具。

## 文档更新（Docs to Update）

- `README.md`：更新 latest 策略与 backfill 行为说明。

## 实现里程碑（Milestones / Delivery checklist）

- [x] M1: 建立 `docs/specs` 根并新增本规格。
- [x] M2: 修复 `ci.yml` 发布顺序、push 门禁、latest 判定与 timeout。
- [x] M3: 修复 `release-backfill.yml` 对齐主链路行为。
- [x] M4: 更新 `README.md` 发布口径并通过本地静态校验。
- [ ] M5: 创建并合并 PR，完成 backfill 与闭环验证报告。

## 方案概述（Approach, high-level）

- 先修 workflow 逻辑正确性（顺序 + 门禁 + latest 判定），再通过 fast-track 进入 PR 与 CI 收敛。
- 合并后按 tag 顺序执行 backfill，并以四项对账（Release/GHCR/pull/Dockrev）出具证据。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：GHCR 查询可能受权限与可见性影响，需要在有权限上下文复核。
- 开放问题：线上 Dockrev API 与主机凭证是否可直接调用（若不可需用户协助执行）。
- 假设：回填 workflow 可在当前仓库权限模型下触发并推送到 GHCR。

## 变更记录（Change log）

- 2026-02-25: 新建规格，冻结发布链路修复目标与验收。
- 2026-02-25: 完成 workflow 与 README 代码改动，进入 PR 与远端验证阶段。

## 参考（References）

- CI run: https://github.com/IvanLi-CN/catnap/actions/runs/22259249496
- Latest release: https://github.com/IvanLi-CN/catnap/releases/tag/v0.2.2
