# 库存历史与近 1 日走势（每分钟）（#0004）

## 状态

- Status: 待实现
- Created: 2026-01-21
- Last: 2026-01-21

## 背景 / 问题陈述

- 现状：系统只展示“当前库存数量 + 最近更新时间”，缺少可追溯的历史走势。
- 痛点：无法快速判断“是否刚补货 / 是否波动频繁 / 是否长期为 0”，需要人工盯刷新与凭感觉判断。
- 目标：为每个配置记录库存历史，并在配置卡片上展示最近 24 小时的走势（minute bucket，X 轴按真实时间比例）。

## 目标 / 非目标

### Goals

- 为每个配置（Config）记录库存数量的历史序列（minute resolution）。
- 在“配置卡片”上展示最近 24 小时走势（X 轴按真实时间比例，不能按“点数”平分横坐标），纵轴固定 0–10（>10 显示为 10+）。
- 图表数据随“库存刷新事件”（轮询或未来 SSE）更新；重启后历史仍可用（在保留期内）。

### Non-goals

- 不做价格历史、规格历史、或跨维度（国家/可用区）的聚合分析。
- 不做多天对比、缩放/拖拽/交互式分析等复杂图表能力。
- 不改变通知/告警策略（补货/价格/配置变化等规则不在本计划内调整）。

## 范围（Scope）

### In scope

- 后端：
  - 采样：在每次“库存刷新事件”发生时写入（minute bucket，对齐到分钟；不要求每分钟都写入）。
  - 存储：SQLite 新增库存历史表与清理策略。
  - 查询：提供批量查询最近 24 小时走势的 API（供 UI 一次性取回页面所需数据）。
- 前端：
  - 在配置卡片上渲染“近 24h 库存走势”迷你折线图（sparkline），并在“库存监控 / 全部产品”两处展示。
  - 纵轴范围固定为 0–10，并对超出范围的值按既定规则处理（见开放问题）。

### Out of scope

- 将历史数据导出、或提供面向外部的公开接口。
- 为历史数据提供独立的设置页面（例如自定义窗口/采样粒度/保留期）。
- 图表交互（tooltip、缩放、拖拽、下载图片等）。

## 需求（Requirements）

### MUST

- 数据采样
  - 每个配置都能产生“minute bucket”对齐的库存历史（至少覆盖最近 24 小时；保留期内可追溯更长时间）。
  - 历史点包含：分钟时间戳（minute bucket）与库存数量（raw quantity，>=0）。
- 数据查询
  - UI 可通过一个 API 请求拿到“页面上多张配置卡片”的近 24h 序列数据。
  - 返回数据可直接用于按真实时间比例绘制走势（必须包含每个点的时间戳，避免把点数当作均匀采样）。
- UI 展示
  - 配置卡片展示近 24h 走势（minute bucket），作为卡片背景层渲染（不额外占用布局空间；不单独占一块“图表区域/标题行”），X 轴按真实时间比例，纵轴固定 0–10（>10 显示为 10+）。
  - 走势样式为“填充折线”（area）：折线 + 半透明填充，避免纯折线过细导致背景存在感不足。
  - 卡片底部状态徽章（例如“库存”“更新”）必须为内容宽度（fit-content），不得拉伸成整条横条（避免视觉噪音与误解为进度条）。
  - 徽章背景必须半透明（让背景走势可见），同时保持文字可读性。
  - 徽章行应靠近卡片底部（保持紧凑底边距），避免底部出现明显更大的留白。
  - 当数据不足/无数据时，卡片以一致的占位态表达（例如空图/灰线/提示文案）。
- 可靠性与性能
  - 历史写入与查询失败不应影响“当前库存”功能可用性（失败降级为不显示走势图，但页面可用）。
  - 单页加载的走势图查询在合理数据量下应可接受（避免 N+1 请求）。

## 接口契约（Interfaces & Contracts）

本计划新增接口与 DB schema；契约以本目录下文档为准，并与 #0001（基础库存监控）的契约保持兼容。

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Inventory history（batch query） | HTTP API | internal | New | ./contracts/http-apis.md | backend | web UI | 批量按 configIds 查询近 24h（minute）走势 |
| Inventory samples（minute history） | DB | internal | New | ./contracts/db.md | backend | backend | SQLite 新表；支持写入与按窗口查询 |

### 契约文档（按 Kind 拆分）

- [contracts/README.md](./contracts/README.md)
- [contracts/http-apis.md](./contracts/http-apis.md)
- [contracts/db.md](./contracts/db.md)

## 验收标准（Acceptance Criteria）

- Given 系统对某配置已产生至少 3 次“库存刷新事件”（轮询或手动刷新均可）
  When 每次刷新事件完成后写入历史样本
  Then 该配置在历史表中至少存在 3 条记录，且每条记录的时间戳按分钟对齐（minute bucket）

- Given 某配置库存在 24 小时内发生过变化（例如 0→3→0）
  When UI 加载包含该配置的页面并请求“近 24h 走势”
  Then 配置卡片以“背景层”展示走势线，且走势与历史数据一致（包含变化发生的分钟），并且不会挤占/改变卡片原有信息布局（不出现单独的图表区域；卡片高度不变）

- Given 配置卡片显示“库存 / 更新”等徽章
  When 页面渲染卡片
  Then 徽章宽度随内容自适应（fit-content），不会拉伸到占据半张卡片或整行宽度

- Given 某配置在窗口内没有任何历史数据
  When UI 请求“近 24h 走势”
  Then 该配置返回可识别的“无数据”响应，且 UI 以占位态展示，不出现 JS 错误

- Given 库存数量超出 10（若确实存在）
  When 绘制走势图
  Then 纵轴仍固定 0–10，并按冻结的规则处理超出值（例如截断并标识）

## UI 设计（Wireframes）

- `docs/plan/0004:inventory-history-trend/ui/README.md`
- `docs/plan/0004:inventory-history-trend/ui/inventory-monitor-with-trend.svg`
- `docs/plan/0004:inventory-history-trend/ui/products-with-trend.svg`

## 实现前置条件（Definition of Ready / Preconditions）

- 已冻结：
  - 采样规则（在“库存刷新事件”时写入 minute bucket；不强制每分钟写入）
  - 返回形状：sparse points（每个点带 `tsMinute`，UI 按真实时间比例映射 X 轴）
  - 超出 0–10 的处理策略
  - 历史保留期与清理策略
- 已完成最小必要 repo reconnaissance：
  - 后端 API router：`src/app_api.rs`（`/api/*`）
  - 后端 DB 访问：`src/db.rs`
  - 后端轮询与刷新：`src/poller.rs`（未来 SSE 可替换触发源，但“写入历史 + 查询接口”保持）
  - 前端 UI：`web/src/App.tsx`（route `monitoring` / `products` 均有配置卡片渲染）

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit tests（Rust）：
  - 分钟桶对齐逻辑（时间截断、窗口边界）
  - 补齐逻辑（缺失分钟的 fill 规则）
  - 清理策略（按保留期删除旧数据）
- Integration tests（Rust）：
  - 写入与查询 roundtrip（SQLite）

### UI / Storybook (if applicable)

- 若后续引入/已有 Storybook：为“配置卡片（含走势）”补齐 story（本计划不单独引入新工具）

### Quality checks

- `cargo fmt --all`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features`
- `web`: `bun run lint` + `bun run typecheck`

## 文档更新（Docs to Update）

- `docs/plan/0001:lazycats-cart-inventory-monitor/contracts/db.md`: 在实现阶段把“库存历史表”以增量变更方式同步进去，避免 DB 口径分裂
- `docs/plan/0001:lazycats-cart-inventory-monitor/contracts/http-apis.md`: 若决定复用/扩展既有 endpoint（而非新增 endpoint），需同步更新该契约

## 实现里程碑（Milestones）

- [ ] M1: DB：新增 `inventory_samples_1m` 并写入 minute bucket（全量 configs + upsert）
- [ ] M2: API：实现 `POST /api/inventory/history`（sparse points，rolling 24h）+ 测试（窗口/排序/空序列）
- [ ] M3: UI：配置卡片 sparkline（库存监控 + 全部产品），X 轴按真实时间比例，>10 显示 10+
- [ ] M4: 保留期：按 30 天清理旧样本（含测试与失败降级策略）

## 方案概述（Approach, high-level）

- 数据采样与存储
  - 基于现有抓取/轮询结果，把每次 `checked_at` 归一化到“分钟桶”（minute bucket）并写入历史表（幂等 upsert）。
  - 历史表只保留窗口所需 + buffer（保留期由决策冻结），并提供简单的清理策略（周期性 delete）。
- 查询接口
  - 提供批量查询接口：输入 configIds，返回每个 config 的近 24h minute 序列（数据形状与补齐规则在契约中冻结）。
- UI 展示
  - 卡片内渲染轻量 sparkline；数据不足时降级为占位态，不阻断页面主流程。
  - 渲染方式：优先使用自研 SVG sparkline（避免引入图表库依赖），按真实时间比例映射 X 轴。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：
  - minute 粒度数据量增长较快：需明确保留期与清理策略，避免 SQLite 文件膨胀影响查询性能。
  - 轮询间隔可配置：若 interval > 1min，如何满足“每分钟走势”的口径需要决策（补齐/独立采样）。

### 已确认的决策（Decisions）

- 本计划（#0004）实现基线：以 `main`（当前为 `v0.1.4`）为准，在其上增量实现“历史库存 + 近 24h 走势”。
- 走势展示范围：包含“库存监控”与“全部产品”。
- 历史采样覆盖范围：所有 Config 都采样。
- 轮询间隔（poll interval）> 1min：允许历史点更稀疏；UI 必须按时间比例展示（不做插值）；未来改 SSE 推送时同样按“刷新事件”写入。
- API 返回形状：sparse points（每个点都带 `tsMinute`），UI 按真实时间比例映射 X 轴。
- “最近一天”：按滚动 24 小时（rolling 24h）定义（非自然日）。
- 历史保留期：暂定 30 天（实现按此口径落地，后续如需可配置另开计划）。
- 纵轴 0–10：quantity > 10 时按 `10+` 显示。
- 图表实现：不引入第三方图表库；使用 SVG sparkline（可用 step line/staircase 表达离散 quantity）。

### 仍需主人确认（Open questions）

- None.

### 假设（Assumptions）

- None.

## 变更记录（Change log）

- 2026-01-21: 创建计划并冻结口径（API=sparse，rolling 24h，保留 30 天，>10=10+），状态切换为 `待实现`
- 2026-01-21: UI 细化：背景 area 走势、徽章半透明、徽章与按钮紧凑与对齐

## 参考（References）

- `docs/plan/0001:lazycats-cart-inventory-monitor/PLAN.md`
- `docs/plan/0001:lazycats-cart-inventory-monitor/contracts/db.md`
- `docs/plan/0001:lazycats-cart-inventory-monitor/contracts/http-apis.md`
