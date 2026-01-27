# 采集观测台：全局采集队列 + SSE 日志订阅（#0011）

## 状态

- Status: 部分完成（4/5）
- Created: 2026-01-25
- Last: 2026-01-27

## 背景 / 问题陈述

- 当前“日志”页是按用户隔离的历史事件分页（`GET /api/logs`），无法呈现“全局采集队列 / worker 状态 / 成功率 / 触发原因聚合 / 实时 tail”。
- 目标站点采集在后端执行（`poller` + 手动 refresh job），目前缺少面向普通用户的运行态观测：现在在抓什么、为什么抓、队列积压、成功率、推送触发与结果等。
- 需要新增一个独立页面作为“采集观测台”，以 **SSE** 提供实时状态与日志流，并支持断线续传。

## 目标 / 非目标

### Goals

- 新增一个“采集观测台”页面，面向普通用户可见，用于观察**全局共享**的采集运行态。
- 采集任务粒度固定为 `(fid,gid)`（同一可用区页面抓取 + 解析）。
- 页面可展示：
  - 当前队列状态（pending/running/成功/失败聚合）、任务列表、任务“发起原因”聚合（不暴露具体用户）。
  - worker（并发执行者）状态：当前在跑的任务、耗时、最近错误。
  - 成功率：仅“抓取+解析成功率”，按窗口选择（24h/7d/30d）。
  - 推送成功率：Telegram/Web Push 分渠道统计（与成功率口径分离）。
  - 采集量（Volume）：当前窗口内任务量与平均速率（由统计推导）。
  - 最近 N 条聚合日志（含成果与推送触发），自动滚动（用户上滚则暂停跟随）。
- 后端使用 SSE 发布实时事件与日志，并支持 `Last-Event-ID` 断线续传（最多回放 1 小时）。

### Non-goals

- 不引入管理员权限/多租户控制台；本计划页面对所有已鉴权用户可见，但不展示触发者 user id 等敏感信息。
- 不做“补跑/回补队列”语义：同一 `(fid,gid)` 任务在 pending/running 时，新增需求只做合并计数，不追加二次执行。
- 不把现有 `/api/logs` 重构为全局日志系统（本计划新增 ops 专用的观测与事件流）。

## 范围（Scope）

### In scope

- 后端：
  - 建模全局队列（按 `(fid,gid)` 去重、合并“发起原因计数”）。
  - 可配置 worker 并发（默认 2），执行“抓取+解析”任务并产生结构化事件。
  - 采集成功率与推送成功率统计（窗口可选 24h/7d/30d）。
  - 新增 HTTP snapshot API：`GET /api/ops/state`。
  - 新增 SSE stream：`GET /api/ops/stream`（支持 `Last-Event-ID` 回放 1 小时内事件）。
  - 落库 7 天的 ops 事件/日志与任务运行记录，用于：
    - snapshot 返回最近 N 条日志；
    - 成功率/渠道成功率按窗口统计；
    - SSE 断线续传（仅保证 1 小时窗口）。
  - 日志必须包含：
    - 任务 start/end、抓取结果（HTTP/耗时/大小）、解析结果；
    - “成果”（restock/price/config 等）与推送触发/发送结果（成功与失败都记录）。
  - 解析成功口径：抓取成功且解析阶段不报错即成功；解析器不得“静默吞掉无法解析”的情况（必须显式报错并记录）。
- 前端：
  - 新增页面入口（建议新增路由 `#ops`），展示 queue/workers/stats/log tail。
  - SSE 订阅 `/api/ops/stream`，支持 `Last-Event-ID` 续传；断线/回放失败时自动 reset（重新拉 snapshot）。
  - 日志 tail：最近 N 条、自动滚动、上滚暂停/提供“回到底部/恢复跟随”。
  - 窗口选择：24h/7d/30d（影响成功率与推送成功率统计）。

### Out of scope

- 目标站点抓取的高并发优化与激进节流策略调整（本计划只引入 worker 并发的可配置化与安全默认值）。
- 将所有后端 tracing 日志完整导出到 UI（仅 ops 相关结构化事件与聚合日志）。

## 需求（Requirements）

### MUST

- 任务粒度固定为 `(fid,gid)`，全局共享队列去重：
  - 若同 key 已在 pending/running，新需求只累计合并计数（按原因类型计数）。
  - 不追加二次执行（不做补跑）。
- 任务必须记录发起原因（聚合计数），至少覆盖：
  - `poller_due`（轮询到点）
  - `manual_refresh`（手动触发 refresh job 派生的 region 任务）
  - `manual_ops`（ops 页面/接口触发的显式 enqueue，如实现需要）
- 可配置 worker 并发，默认 2；在 ops 页面可见当前 worker 状态与正在处理的任务。
- 成功率口径与窗口：
  - 成功率仅统计“抓取+解析成功”；
  - 窗口可选：24 小时 / 最近一周 / 最近一月（影响 snapshot 与 SSE metrics 更新）。
- 推送成功率：
  - Telegram 与 Web Push 分渠道统计；
  - 发送成功与失败均必须进入 ops 日志（并计入渠道成功率分子/分母）。
- 新增 snapshot API：`GET /api/ops/state?range=24h|7d|30d`，返回：
  - queue 概览、workers 列表、任务列表（可截断）、成功率与渠道成功率、最近 N 条日志 tail、server time、replay window 等。
- 新增 SSE：`GET /api/ops/stream?range=24h|7d|30d`，并支持：
  - `Last-Event-ID` 回放（最多 1 小时窗口）；若 `Last-Event-ID` 过旧或非法，必须向客户端发出 reset 信号并引导其重新拉 snapshot。
  - 事件必须有单调递增 `id`（用于续传与客户端去重），并按 id 有序投递（at-least-once）。
- ops 日志保留至少 7 天；SSE 回放只保证 1 小时内（更久不承诺）。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Ops state snapshot | HTTP API | internal | New | ./contracts/http-apis.md | backend | web | `GET /api/ops/state` |
| Ops stream | HTTP API / Event (SSE) | internal | New | ./contracts/http-apis.md / ./contracts/events.md | backend | web | `GET /api/ops/stream` |
| Ops event schemas | Event (SSE) | internal | New | ./contracts/events.md | backend | web | 事件名/载荷/续传语义 |
| Ops persistence | DB | internal | New | ./contracts/db.md | backend | backend | 任务运行/通知/事件存储（7d） |
| Ops runtime knobs | Config | internal | New | ./contracts/config.md | backend | deploy | 默认并发=2、回放窗口=1h 等 |
| Ops UI route | UI Component | internal | New | None | web | user | 建议 `#ops`；不单独出 props 契约 |

### 契约文档（按 Kind 拆分）

- [contracts/README.md](./contracts/README.md)
- [contracts/http-apis.md](./contracts/http-apis.md)
- [contracts/events.md](./contracts/events.md)
- [contracts/db.md](./contracts/db.md)
- [contracts/config.md](./contracts/config.md)

## 代码位置（Repo reconnaissance）

本计划预计触及/复用的关键入口点（现状扫描）：

- 后端路由：`src/app_api.rs`（新增 `GET /api/ops/state`、`GET /api/ops/stream`）
- 轮询调度：`src/poller.rs`（当前为 per-user 循环；后续将向全局队列 enqueue `(fid,gid)` 任务）
- 上游抓取与解析：`src/upstream.rs`（`fetch_region_configs()` + `parse_*()`；需要保证解析异常显式报错）
- 数据库存储与清理：`src/db.rs`（新增 ops 表与 7 天清理；可复用现有清理 loop 触发点）
- 前端 UI：`web/src/App.tsx`（当前为单文件路由与数据获取；新增页面与 SSE 订阅逻辑需落在 web 侧）

## 验收标准（Acceptance Criteria）

- Given 系统已运行且用户已鉴权
  When 打开 `#ops` 页面
  Then 5 秒内可看到 snapshot（queue/workers/stats/log tail），且页面不依赖手动刷新才能更新状态。
- Given `(fid,gid)` 任务正在执行
  When 任务开始与结束
  Then 前端通过 SSE 收到对应事件（start/end），并在 worker 面板反映状态变化，日志 tail 追加记录。
- Given 断网或刷新页面导致 SSE 断开
  When 客户端携带 `Last-Event-ID` 重连且该 id 在 1 小时回放窗口内
  Then 客户端收到缺失事件的回放，且不产生重复渲染（按 id 去重）。
- Given 客户端携带的 `Last-Event-ID` 过旧（超出 1 小时窗口）或非法
  When 建立 SSE 连接
  Then 服务端发送 reset 信号，客户端自动重新拉取 snapshot 并恢复订阅。
- Given 目标站点 HTML 结构异常导致解析无法进行
  When 执行 `(fid,gid)` 任务
  Then 该次运行计为失败（影响成功率），并在日志中包含可定位的错误信息（不允许静默成功或空结果不报错）。
- Given 发现成果事件（例如 restock/price/config）
  When 触发通知
  Then ops 日志中必须出现“成果”记录与推送触发记录，并记录每个渠道的发送结果（成功/失败）。
- Given 在页面切换统计窗口（24h/7d/30d）
  When 选择窗口并生效
  Then 成功率与渠道成功率按选定窗口刷新，且 SSE 推送的 metrics 使用同一窗口口径。

- Given 本计划的 UI 设计图与设计文档已产出在 `docs/plan/0011:ops-collection-dashboard/ui/`
  When 实现完成并准备交付
  Then 项目内不再存在对 `docs/plan/` 的运行/交付依赖，且下方 Asset promotion 已晋升到目标位置并完成引用更新。

## 实现前置条件（Definition of Ready / Preconditions）

- 本 `PLAN.md` 与契约文档已评审并冻结（接口字段与事件语义无需在实现阶段再改口径）。
- 已确认“解析失败”应显式报错（不得静默忽略）。
- 已确认：worker 并发默认 2、可配置；日志保留 7 天；SSE 回放 1 小时。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Unit tests:
  - 事件 id 单调性、回放窗口判定、队列合并计数规则。
- Integration tests:
  - `GET /api/ops/state` 响应 schema 与边界参数（range、limit）。
  - `GET /api/ops/stream`：
    - `content-type: text/event-stream`；
    - `Last-Event-ID` 回放与 reset 行为。
- E2E tests (if applicable):
  - Web：Ops 页面能订阅 SSE 并渲染 tail（可复用现有 Storybook+Playwright 测试框架）。

### UI / Storybook (if applicable)

- 新增/更新 story：Ops 页面（含不同状态：空队列/有任务/有错误/断线重连）。

### Quality checks

- Rust：`cargo fmt`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo test --all-features`
- Web：`cd web && bun run lint`、`cd web && bun run typecheck`、`cd web && bun run test:storybook`（如新增 stories）

## 文档更新（Docs to Update）

- `README.md`: 增加 Ops 页面入口（路由）与相关环境变量说明（仅摘要，不重复契约细节）。
- `docs/plan/README.md`: 本计划索引行与状态推进。
- `docs/ui/README.md`: 在实现完成后晋升 UI 设计图（见下方 Asset promotion）。

## 资产晋升（Asset promotion）

| Asset | Plan source (path) | Used by (runtime/test/docs) | Promote method (copy/derive/export) | Target (project path) | References to update | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| Ops 页面线框图（SVG） | `docs/plan/0011:ops-collection-dashboard/ui/ops-dashboard.svg` | docs | copy | `docs/ui/ops-dashboard.svg` | `docs/ui/README.md` | 以“最新版本 UI 设计图”晋升到项目级目录（参照 `docs/ui/README.md` 约定） |
| SSE 状态悬浮气泡（SVG） | `docs/plan/0011:ops-collection-dashboard/ui/sse-tooltip.svg` | docs | copy | `docs/ui/sse-tooltip.svg` | `docs/ui/README.md` | 仅用于解释“状态点 + 详情气泡”，不作为页面主体组成部分 |

## 实现里程碑（Milestones）

- [x] M1: 后端：全局队列 + worker 池 + ops DB 留存（7d）+ snapshot API
- [x] M2: 后端：SSE stream（含 Last-Event-ID 回放 1h + reset 语义）+ 事件/日志产出覆盖成果与推送
- [x] M3: 前端：Ops 页面 + SSE 订阅 + log tail 自动滚动 + range 切换
- [x] M4: 测试与文档：集成测试补齐 + README 更新
- [ ] M5: 交付确认：UI 与 `docs/ui/ops-dashboard.svg` 一致（owner check）

## 方案概述（Approach, high-level）

- 将“采集任务”从直接调用上游抓取，收敛为“向全局队列 enqueue `(fid,gid)`”，由 worker 池拉取执行：
  - poller 与手动 refresh job 都只负责产生需求（enqueue + 原因计数），不直接抓取。
  - 同 `(fid,gid)` 在 pending/running 时合并原因计数，避免排队膨胀。
- ops 事件体系：
  - 所有关键状态变化（enqueue/start/end/error/notify）都产生结构化事件；
  - 事件同时用于：SSE 推送、snapshot tail、统计口径（成功率/渠道成功率）。
- SSE 续传：
  - 事件 id 由 DB 自增生成，确保单调递增；
  - 回放窗口强制为 1 小时：超过窗口则发送 reset，引导客户端重新拉 snapshot。
- 解析失败处理：
  - 对“结构异常/无法解析”必须显式报错并计入失败；不允许以空列表“假装成功”。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：
  - 全局队列对上游压力更可控，但需要明确并发与节流策略（默认并发=2，必要时可降为 1）。
  - 解析失败口径若过于严格，可能把“合法空页”判为失败；需在实现时基于目标站点页面确认空态锚点（必要时加入可配置的空态判定）。
- 假设（需主人确认）：
  - 统计窗口的定义按“结束时间（serverTime）向前回溯”计算（24h=最近 24*60*60 秒；7d/30d 同理）。

## 变更记录（Change log）

- 2026-01-25: 创建计划，冻结范围、接口与验收标准。
- 2026-01-25: UI 调整：replay/Last-Event-ID 移至 SSE 状态悬浮气泡；KPI 区补充 Volume 卡片。
- 2026-01-25: UI 调整：页面文案统一中文；补充 `ui/sse-tooltip.svg` 作为悬浮气泡独立设计图。
- 2026-01-26: 实现落地：全局队列 + `/api/ops/state|stream` + Ops 页面（#ops）+ 7d 留存与统计；UI 资产晋升到 `docs/ui/`。
- 2026-01-26: UI 对齐：按 `docs/ui/ops-dashboard.svg` 重做布局（顶栏控件/KPI 卡片/区块/日志 tail），snapshot 增加 `sparks` 用于 sparkline；待主人确认后标记“已完成”。
- 2026-01-26: UI 微调：KPI 卡片边框/光晕/顶部条圆角对齐 `docs/ui/ops-dashboard.svg`；本地预览补齐常驻启动脚本（避免 review 时服务退出）。
- 2026-01-27: UI 修复：深色主题下按 `docs/ui/ops-dashboard.svg` 对齐背景层级（main surface）与 KPI glow（radial），并增强 Storybook 常驻脚本稳定性；待主人确认后再勾选 M5。

## 参考（References）

- 现有用户隔离日志页：`GET /api/logs`（仅用于对比，不在本计划内扩展为全局观测）。
- 相关计划：`0009:catalog-full-refresh-sse`（同样涉及 SSE 与全局共享语义，本计划优先复用其基础设施而不改动其口径）。
