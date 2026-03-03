# 配置卡片点击打开下单页（#4tnv8）

## 状态

- Status: 部分完成（3/4）
- Created: 2026-03-03
- Last: 2026-03-03

## 背景 / 问题陈述

当前“全部产品/库存监控”中的配置卡片只能用于查看与监控开关操作，缺少从配置卡片直接进入上游下单页面的能力，导致用户需要手动回到上游站点重新定位配置。

## 目标 / 非目标

### Goals

- 在“全部产品 + 库存监控”两类卡片支持点击打开下单页（新标签页）。
- 后端显式透传 `sourcePid`，前端基于 `catalog.source.url + sourcePid` 构造下单链接。
- 监控开关按钮与卡片跳转解耦：点击开关只切换监控状态，不触发跳转。
- 无链接时给出“暂无下单链接”提示，且卡片不可跳转。

### Non-goals

- 不实现自动下单、支付或结算流程自动化。
- 不调整库存抓取、通知策略或监控策略。
- 不引入新的全局 toast/弹窗系统。

## 范围（Scope）

### In scope

- API 字段扩展：`ConfigView.sourcePid`（可选）。
- Products/Monitoring 卡片跳转行为、键盘可达性、focus-visible 样式。
- Storybook 场景覆盖可跳转与不可跳转态。

### Out of scope

- 上游站点接口改造。
- 与下单页交互后的流程控制。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `ConfigView.sourcePid` | HTTP JSON field | internal | Modify | None | backend | web | 可选字段，缺失时不序列化 |

## 验收标准（Acceptance Criteria）

- Given 配置存在 `sourcePid`
  When 用户点击“全部产品”卡片
  Then 新标签页打开 `.../cart?action=configureproduct&pid=<pid>`。

- Given 配置存在 `sourcePid`
  When 用户点击“库存监控”（含最近 24h 与分组卡片）卡片
  Then 跳转行为与“全部产品”一致。

- Given 用户点击 `ProductCard` 内监控开关按钮
  When 卡片本身可跳转
  Then 仅触发监控开关逻辑，不触发跳转。

- Given 配置缺失 `sourcePid`
  When 用户查看卡片
  Then 卡片不可跳转，并展示“暂无下单链接”提示。

- Given 卡片可跳转
  When 用户使用键盘聚焦并按 `Enter` 或 `Space`
  Then 在新标签页打开下单链接，且焦点样式可见。

## 非功能性验收 / 质量门槛（Quality Gates）

- Backend: `cargo test --all-features`
- Web: `bun run lint` + `bun run typecheck` + `bun run test:storybook`
- 不引入布局回退或卡片交互冲突。

## 实现里程碑（Milestones）

- [x] M1: `ConfigView` 新增可选 `sourcePid`，并在 `/api/bootstrap`、`/api/products`、`/api/monitoring` 返回
- [x] M2: Products + Monitoring 卡片支持点击/键盘跳转；监控开关与跳转解耦
- [x] M3: 缺失链接提示、样式与 Storybook 场景补齐
- [ ] M4: 本地验证通过并进入 fast-track 收敛

## 风险 / 假设

- 风险：历史数据中存在 `source_pid` 为空的配置，会进入“不可跳转”降级态。
- 假设：上游下单路径保持 `cart?action=configureproduct&pid=<pid>`。

## 变更记录（Change log）

- 2026-03-03: 初始化规格，冻结“卡片跳转下单页”范围与验收口径。
- 2026-03-03: 完成 API `sourcePid` 透传、Products/Monitoring 跳转交互、Storybook 交互用例与本地验证；待 PR 阶段收敛。
