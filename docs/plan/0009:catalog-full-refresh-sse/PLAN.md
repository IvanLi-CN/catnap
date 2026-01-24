# 全量刷新：SSE 进度 + 缓存复用 + 配置上下架（#0009）

## 状态

- Status: 待实现
- Created: 2026-01-23
- Last: 2026-01-24

## 背景 / 问题陈述

- “全部产品”页目前只有“刷新：手动”（仅重新拉取 `/api/bootstrap`），无法确保触发**全量**抓取上游来发现新配置。
- “监控”页的“重新同步”使用轮询式 `GET /api/refresh/status` 展示进度，无法覆盖“系统自动刷新”的状态展示，也不满足“服务端 SSE 实时发布”。
- 当前监控抓取只更新已存在的配置；同一可用区页面出现新配置/下架配置时无法在 DB 与 UI 中体现。
- 全量抓取与监控抓取目前各自发请求；缺少“按 URL 复用子任务 + 缓存 last good result”的策略，容易重复拉取或在失败时无回退。

## 目标 / 非目标

### Goals

- 在“全部产品”页将按钮改名为“立即刷新”，且其行为为**全量刷新**（发现新配置、识别下架配置）。
- 支持“每 N 小时自动全量刷新一次”，N 可在“系统设置”中配置，默认 6 小时，可关闭。
- 刷新进度与状态由服务端通过 **SSE** 实时发布；无论手动触发还是自动触发，页面都能展示“正在刷新”的视觉状态。
- 全量刷新与监控抓取共享“页面级（URL 级）的子获取任务”：可复用 in-flight、可按策略复用缓存，且每个 URL 保留 last good result 备用。
- 在抓取结果对比 DB 差异后：
  - 新出现配置：写入 DB（标记上架），出现在 UI 中。
  - 消失配置：在 DB 标记下架（不删除），UI 给予“下架”标记。
- 新增“上架监控 / 下架监控”的开关（分别配置是否启用）。

### Non-goals

- 不引入通用的多租户调度/权限模型；本计划仅覆盖当前用户隔离与现有鉴权机制。
- 不做“多版本历史缓存/回溯”能力：缓存仅要求“每 URL 的最后一个成功版本（last good result）”。
- 不承诺上游 HTML 结构变更下的零维护；仅提供失败回退与可观测性。

## 范围（Scope）

### In scope

- 后端：
  - 全量刷新 job（可手动触发 + 可按设置自动触发）。
  - SSE 事件流发布刷新状态/进度（并包含每个 URL 子任务的进度）。
  - URL 级抓取任务的去重、缓存与复用策略（监控/全量共享）。
  - 配置生命周期（上架/下架/重新上架）差异计算与 DB 标记。
  - 上架/下架监控开关（与通知链路集成：log + Telegram/Web Push，如已启用）。
- 前端：
  - “全部产品”页按钮改为“立即刷新”，并显示刷新状态（SSE 驱动）。
  - 配置卡片展示“下架”标记（未下架不展示）。
  - “系统设置”增加：自动全量刷新间隔、上架监控开关、下架监控开关。
- 测试：
  - 生命周期差异计算、缓存复用策略的单元测试。
  - API（含 SSE）与 DB 标记的集成测试（Rust integration tests）。
  - Storybook stories 更新（Products/Monitoring/Settings 页）。

### Out of scope

- 变更上游抓取策略以实现高并发；并发/节流只在现有基础上做安全调整。
- 将“监控轮询”从 per-user 改为全局统一调度（除非为满足自动全量刷新不可避免）。

## 需求（Requirements）

### MUST

- “全部产品”页的刷新按钮文本为“立即刷新”，点击后触发**全量刷新**（而非仅 reload bootstrap）。
- 自动全量刷新：
  - 默认每 6 小时触发一次；
  - 可在 UI 配置间隔（小时）；
  - 可关闭自动全量刷新。
- 多用户语义：
  - 站点一致，catalog 抓取与全量刷新调度为**全局共享**。
  - 自动全量刷新间隔来源于用户设置，系统取所有用户里“启用的间隔”的**最小值**作为全局调度间隔。
- 服务端提供 SSE（`text/event-stream`）实时推送刷新状态，至少包含：
  - job 状态（idle/running/success/error）
  - 触发来源（manual/auto）
  - 进度（done/total）
  - 当前子任务信息（URL key、是否使用缓存/是否发起抓取）
- 全量刷新与监控抓取共享“URL 子任务”：
  - 同一 URL 并发请求必须去重（in-flight 复用）。
  - 支持按策略跳过抓取并使用缓存（last good result）。
  - 每个 URL 必须保留 last good result 备用（可跨进程重启的持久化）。
- 发现新配置/下架配置：
  - 以“同一 URL 返回的配置集合”与 DB 记录做差异对比。
  - 下架判定：一次成功抓取缺失即标记下架（不做宽限期）。
  - 下架配置只标记状态，不删除。
  - UI 需要显示下架标记。
- “系统设置”增加并持久化：
  - 自动全量刷新间隔（小时，支持关闭）
  - 上架监控是否启用
  - 下架监控是否启用
- 通知：
  - 当“上架监控 / 下架监控”启用时，发生对应事件需产生通知（并写入日志）；通知对象为“所有启用该开关的用户”；通知渠道沿用现有 Telegram/Web Push 的启用状态与配置。
- 监控页增强：
  - 监控页面顶部新增区域，展示最近 24 小时“上架（listed，含重新上架）”的产品/配置列表（用于快速发现新服务）。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Catalog refresh trigger | HTTP API | internal | New | ./contracts/http-apis.md | backend | web | “立即刷新”与自动刷新共用 |
| Catalog refresh status stream | Event (SSE) | internal | New | ./contracts/events.md | backend | web | 刷新状态由 SSE 实时推送 |
| Monitoring list response | HTTP API | internal | Modify | ./contracts/http-apis.md | backend | web | 增加 `recentListed24h` |
| Settings fields | HTTP API / DB | internal | Modify | ./contracts/http-apis.md / ./contracts/db.md | backend | web | 新增自动刷新与上/下架监控开关 |
| Config lifecycle fields | HTTP API / DB | internal | Modify | ./contracts/http-apis.md / ./contracts/db.md | backend | web | config 增加 lifecycle（下架标记等） |
| URL last-good cache | DB | internal | New | ./contracts/db.md | backend | backend | 记录每个 URL 的 last good result（config ids） |

### 契约文档（按 Kind 拆分）

- [contracts/README.md](./contracts/README.md)
- [contracts/http-apis.md](./contracts/http-apis.md)
- [contracts/events.md](./contracts/events.md)
- [contracts/db.md](./contracts/db.md)

## 代码位置（Repo reconnaissance）

为避免实现阶段“边翻代码边改口径”，本计划涉及的关键入口点如下（均为现状扫描结果）：

- 后端路由与当前“重新同步”实现：`src/app_api.rs`
  - 现有 endpoints：`POST /api/refresh` + `GET /api/refresh/status`（前端轮询使用）
  - 当前 refresh job：`run_refresh_job()`（支持 full catalog 或按 region 子任务刷新）
- 现有监控轮询与通知：`src/poller.rs`（按用户 settings 轮询；仅对已启用监控的配置抓取）
- 上游抓取与解析：`src/upstream.rs`（`fetch_catalog()` / `fetch_region_configs()`）
- 数据库 schema + settings：`src/db.rs`（`init_db()` 内联建表；`settings`/`catalog_configs` 等表在此定义与更新）
- API 结构体：`src/models.rs`（`SettingsView` / `BootstrapResponse` / `RefreshStatusResponse` 等）
- 前端刷新按钮与轮询：`web/src/App.tsx`
  - 当前实现：点击触发 `POST /api/refresh`，并轮询 `GET /api/refresh/status`（800ms）

## 验收标准（Acceptance Criteria）

- Given 用户在“全部产品”页
  When 点击“立即刷新”
  Then 服务端启动一次全量刷新 job，并通过 SSE 推送 running 状态与进度；按钮进入“刷新中”视觉态，直到 job success/error。

- Given 系统设置中“自动全量刷新间隔=6h（默认）”
  When 到达下一次触发时间
  Then 服务端自动启动全量刷新 job，且所有已打开页面能通过 SSE 看到刷新进行中与完成状态（无须用户点击按钮）。

- Given 监控轮询刚刚完成并成功抓取了 URL=A（同一可用区页面）
  When 全量刷新 job 运行到 URL=A 的子任务
  Then 若该 URL 最近一次成功抓取距今 ≤5 分钟，系统可复用缓存（不必重复抓取），但刷新 job 的进度仍正确推进，并明确标注“cache hit”。

- Given 上游某 URL 返回新增配置 X
  When 任一成功抓取（监控抓取或全量抓取）完成并做差异对比
  Then DB 中新增 X（标记为 active/上架），UI 能展示 X；若“上架监控”启用，则产生一条 log/通知事件。

- Given 上游某 URL 不再返回配置 Y（此前存在且 active）
  When 任一成功抓取完成并做差异对比
  Then DB 将 Y 标记为 delisted/下架（不删除）；UI 对 Y 显示“下架”标记；若“下架监控”启用，则产生一条 log/通知事件。

- Given 用户打开“库存监控”页面
  When 过去 24 小时内有配置发生上架（listed，含重新上架）
  Then 页面顶部“最近 24 小时上架”区域展示这些配置（至少展示名称 + 国家/可用区/价格等卡片必要信息）。

## 实现前置条件（Definition of Ready / Preconditions）

- SSE 事件 payload 形状已确认（contracts 已定稿）。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit tests: lifecycle diff（listed/delisted/relisted）、URL cache hit/miss 策略
- Integration tests: new DB columns/tables + 全量刷新 job 状态推进 + SSE 基础可用性（至少验证 endpoint 行为与格式）

### UI / Storybook (if applicable)

- Stories to add/update:
  - Pages/ProductsView（按钮“立即刷新” + 下架标记）
  - Pages/MonitoringView（可选：展示同一刷新状态组件）
  - Pages/SettingsView（自动刷新与上/下架监控开关）

### Quality checks

- Rust: `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-features`
- Web: `cd web && bun run lint`, `cd web && bun run typecheck`, `cd web && bun run test:storybook`（如覆盖到）

## 文档更新（Docs to Update）

- `docs/plan/README.md`: 增加本计划索引行
- `docs/plan/0009:catalog-full-refresh-sse/ui/README.md`: 本计划 UI 设计说明
- `docs/plan/0009:catalog-full-refresh-sse/ui/products.svg`: 本计划“全部产品”设计图
- `docs/plan/0009:catalog-full-refresh-sse/ui/inventory-monitor.svg`: 本计划“库存监控”设计图
- `docs/plan/0009:catalog-full-refresh-sse/ui/settings.svg`: 本计划“系统设置”设计图
- 实现阶段要求：完成 UI 改动后，将本计划目录内的 UI SVG **同步替换**到项目级集合 `docs/ui/`（并更新 `docs/ui/README.md` 的来源映射），确保工程内“最新设计图”不依赖 plan 目录。

## 实现里程碑（Milestones）

- [ ] M1: 后端全量刷新 job（manual+auto）+ URL 级子任务去重/缓存（含持久化 last good）
- [ ] M2: 生命周期差异（上架/下架/重新上架）+ DB 标记 + 通知开关接入（log + Telegram/Web Push）
- [ ] M3: SSE 状态流 + 前端接入（按钮“立即刷新”/刷新中视觉态/错误提示）
- [ ] M4: UI 下架标记 + 设置页新增配置项 + 完整测试与 Storybook 覆盖

## 方案概述（Approach, high-level）

- 以“URL 子任务”为核心抽象（以 `fid/gid` 归一化为 key，对应上游 cart 页面），监控与全量刷新统一走同一套抓取协调器：
  - in-flight 去重（同 key 同时只抓一次，其余 await 结果）
  - last good result 持久化（DB 记录每个 key 最近一次成功返回的 config id 集合）
  - cache 策略：全量刷新对“最近 5 分钟成功抓取”的 URL 使用 cache hit，超过阈值才发起真实抓取
- 每次成功抓取后做差异计算：
  - `new = fetched - last_seen_for_url`
  - `missing = last_seen_for_url - fetched`
  - 对 `new` 执行 upsert（active/first_seen_at/last_seen_at）；对 `missing` 标记 delisted（不删除）
- SSE 负责发布 job 的状态机与每个 URL 子任务的进度；前端只消费 SSE 状态，不自行推断。
- 全局自动刷新调度：
  - 读取所有用户 settings，取启用的 `autoIntervalHours` 最小值作为全局调度间隔；到点触发一次全量刷新 job，并通过 SSE 广播给所有在线客户端。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：
  - 上游 HTML 结构变动导致解析失败；需确保“失败不污染 last good cache”，并在 SSE/日志中可观测。
  - 下架判定为“一次成功抓取缺失即下架”，可能产生误报；需确保“抓取失败/解析失败”不参与下架判定，只以成功抓取结果做差异。
  - 自动全量刷新与多用户设置的语义可能冲突（catalog 为全局共享）。
- 约定：
  - “全量刷新”允许对部分 URL 使用缓存（cache hit）来减少重复拉取，但仍视为一次“全量刷新 job”。
  - 自动全量刷新：当所有用户都关闭（`autoIntervalHours=null`）时，全局自动刷新不运行（仅允许手动触发）。

## 变更记录（Change log）

- 2026-01-23: 创建计划（待实现）
- 2026-01-24: UI 设计文档与 SVG 收敛到 plan 目录；增加“实现完成后同步替换到 docs/ui/”要求

## 参考（References）

- UI：`docs/plan/0009:catalog-full-refresh-sse/ui/README.md`
- UI：`docs/plan/0009:catalog-full-refresh-sse/ui/products.svg`
- UI：`docs/plan/0009:catalog-full-refresh-sse/ui/inventory-monitor.svg`
- UI：`docs/plan/0009:catalog-full-refresh-sse/ui/settings.svg`
