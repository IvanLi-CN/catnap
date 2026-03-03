# 配置卡片点击打开下单页（#4tnv8）

## 状态

- Status: 已完成
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
- Products 分组标题增加 Iconify link 图标，支持新标签页打开上游国家分组页（`cart?fid=<fid>`）。
- 库存为 0 的卡片点击增加拦截弹窗：先查询最新库存，若恢复有货自动打开；否则允许“仍然打开”忽略限制。

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

- Given 用户点击“全部产品”分组标题旁的 link 图标
  When 分组包含有效 `countryId(fid)`
  Then 新标签页打开 `.../cart?fid=<fid>`（例如芬兰组 `fid=11`）。

- Given 配置卡片当前库存为 0
  When 用户点击卡片
  Then 显示“库存拦截”弹窗并发起最新库存查询；若查询结果库存充足则自动新开下单页，否则保留“仍然打开”按钮供用户忽略限制继续打开。

## 非功能性验收 / 质量门槛（Quality Gates）

- Backend: `cargo test --all-features`
- Web: `bun run lint` + `bun run typecheck` + `bun run test:storybook`
- 不引入布局回退或卡片交互冲突。

## 实现里程碑（Milestones）

- [x] M1: `ConfigView` 新增可选 `sourcePid`，并在 `/api/bootstrap`、`/api/products`、`/api/monitoring` 返回
- [x] M2: Products + Monitoring 卡片支持点击/键盘跳转；监控开关与跳转解耦
- [x] M3: 缺失链接提示、样式与 Storybook 场景补齐
- [x] M4: 下单跳转严格收口到 `configureproduct&pid`，并补齐历史缺失 `sourcePid` 的恢复策略
- [x] M5: “全部产品”分组标题新增 Iconify link 图标，点击新标签页打开 `cart?fid=<fid>`
- [x] M6: 本地验证通过并完成提交（lint/typecheck/storybook + cargo tests）
- [x] M7: 库存为 0 的卡片增加弹窗拦截 + 实时库存复查 + “仍然打开”兜底

## 风险 / 假设

- 风险：历史数据中存在 `source_pid` 为空的配置，会进入“不可跳转”降级态。
- 假设：上游下单路径保持 `cart?action=configureproduct&pid=<pid>`。

## 变更记录（Change log）

- 2026-03-03: 初始化规格，冻结“卡片跳转下单页”范围与验收口径。
- 2026-03-03: 完成 API `sourcePid` 透传、Products/Monitoring 跳转交互、Storybook 交互用例与本地验证；待 PR 阶段收敛。
- 2026-03-03: 用户反馈后将跳转口径严格收口为 `cart?action=configureproduct&pid=<pid>`，去除 `fid/gid` 作为下单页 fallback。
- 2026-03-03: 后端新增 `sourcePid` 缺失恢复（configureproduct 页探测 + 缓存），并在 upsert 时保留既有非空 pid，提升卡片可跳转覆盖率。
- 2026-03-03: 分组标题新增 Iconify link 图标（非按钮样式），支持新标签页直达上游 `cart?fid=<fid>` 页面。
- 2026-03-03: 新增“库存拦截”交互：库存为 0 时先弹窗并实时查询库存；有货自动放行跳转，无货提供“仍然打开”忽略限制按钮。
- 2026-03-03: 调整“库存拦截”弹窗布局为视窗居中显示，保证桌面端与移动端都位于可视区域中心。
