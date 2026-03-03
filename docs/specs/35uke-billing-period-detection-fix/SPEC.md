# 修复付费周期识别：年付被误判为月付（#35uke）

## 状态

- Status: 已完成
- Created: 2026-03-03
- Last: 2026-03-03

## 背景 / 问题陈述

当前上游配置解析在 `src/upstream.rs` 中将 `price.period` 固定写为 `month`。当上游卡片标题或价格文案实际为“年付/元 / 年”时，UI 仍显示为“/ 月”，导致产品价格周期信息错误。

## 目标 / 非目标

### Goals

- 后端按“价格文本优先 + 名称关键词回退”识别付费周期，输出规范值 `month`/`year`。
- 前端 CNY 价格显示正确映射：`month -> 月`、`year -> 年`。
- 增加后端与 Storybook 回归覆盖，防止周期识别再次回归。
- 不改接口字段结构；仅修正字段值语义。

### Non-goals

- 不新增数据库迁移或离线 SQL 回填。
- 不改价格币种显示策略（除周期映射外）。
- 不扩展复杂计费枚举体系。

## 范围（Scope）

### In scope

- `src/upstream.rs`：新增周期识别函数并接入 `parse_configs`。
- `web/src/App.tsx`：增强 CNY 周期本地化映射。
- `tests/fixtures/` + `src/upstream.rs` 测试：增加“元 / 年”“名称年付回退”的覆盖。
- `web/src/stories/**`：增加至少一个“/ 年”可视回归场景。

### Out of scope

- 历史数据离线回填。
- API 字段重命名或结构变更。
- 与付费周期无关的抓取逻辑重构。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `price.period` | HTTP JSON field | internal | Modify | None | backend | web | 字段名与类型不变；值新增稳定 `year` |

## 验收标准（Acceptance Criteria）

- Given 上游价格行包含 `元 / 年`
  When 后端解析配置
  Then `price.period == "year"`。

- Given 上游价格行包含 `元 / 月`
  When 后端解析配置
  Then `price.period == "month"`。

- Given 上游价格行缺失周期但名称包含“年付”
  When 后端解析配置
  Then `price.period == "year"`。

- Given CNY 且 `period=year`
  When 前端渲染价格
  Then 展示 `¥x.xx / 年`。

- Given CNY 且 `period=month`
  When 前端渲染价格
  Then 展示 `¥x.xx / 月`。

## 非功能性验收 / 质量门槛（Quality Gates）

- Web: `bun run lint` + `bun run typecheck` + `bun run test:storybook` + `bun run build`
- Backend: `cargo fmt --check` + `cargo clippy --all-targets --all-features -- -D warnings` + `cargo test --all-features`

## 实现里程碑（Milestones）

- [x] M1: 新增并接入周期识别逻辑（价格文本优先、名称回退）。
- [x] M2: 前端周期展示支持 `year -> 年`，并保持 month 行为不回归。
- [x] M3: 增加后端/Storybook 回归场景并通过质量门禁。
- [x] M4: 创建 PR 并完成 checks + review 收敛。

## 风险 / 假设

- 风险：上游 DOM 结构变化可能影响价格行文本提取精度。
- 假设：全量刷新会覆盖历史 `price_period`，无需单独迁移。

## 变更记录（Change log）

- 2026-03-03: 初始化规格，冻结修复范围、验收标准与质量门禁。
- 2026-03-03: 完成后端周期识别、前端周期映射、后端测试与 Storybook 场景覆盖，并通过本地质量门禁。
- 2026-03-03: 完成 fast-track 收口：push + PR #52 + CI checks 全绿（run #145）+ review-loop 收敛完成。
