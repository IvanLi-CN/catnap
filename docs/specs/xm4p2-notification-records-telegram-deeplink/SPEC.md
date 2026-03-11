# 通知记录页与 Telegram 深链（#xm4p2）

## 状态

- Status: 部分完成（3/4）
- Created: 2026-03-11
- Last: 2026-03-11

## 背景 / 问题陈述

- 现有真实通知只会发送到 Telegram / Web Push，并在技术日志中留下机器可读记录；用户无法在 UI 中回看“某条通知当时到底推了什么”。
- 现有 Telegram 链接仅指向 `#monitoring`，无法直接定位到对应通知，用户从消息跳回 Web UI 后仍需手动翻找。
- 通知记录天然会持续增长，若沿用一次性全量加载，会让页面滚动、定位与长列表渲染很快变得卡顿。

## 目标 / 非目标

### Goals

- 新增独立 `通知记录` 页，按时间倒序展示真实业务通知。
- 明确定义“`1 条通知 = 1 个通知组`”；组内展示该通知关联的机子列表，即使只有 1 项也必须保持列表形态。
- 真实通知发送前先持久化结构化快照；通知页展示通知生成时的机子状态，不追随 catalog 后续变化。
- Telegram 真实通知追加直达通知记录页的深链，并在页面中自动定位、高亮对应通知。
- 通知页必须支持 cursor 分页、按需加载与无限滚动，避免长列表性能退化。

### Non-goals

- 不把多条通知改造成批量通知，也不改变“1 次发送 = 1 条通知记录”的语义。
- 不替换现有技术日志页，不把测试通知写入通知记录页。
- 不修改 Web Push 点击目标或 transport 配置方式。
- 不为通知记录引入全文检索、标签筛选或新的权限模型。

## 范围（Scope）

### In scope

- 后端新增通知记录与通知项快照表、分页/单条查询 API、独立保留策略。
- `poller` 与 `ops` 的真实通知链路改为“构建结构化通知记录 -> 落库 -> 渲染 Telegram/Web Push 文案 -> 发送”。
- Telegram 文案在保留“查看监控”入口的同时，追加“查看通知记录”绝对链接。
- Web 新增 `#notifications` 路由、列表页、深链预取/定位/高亮、无限滚动与缺失态提示。
- Storybook fixtures / stories / 响应式验证更新。

### Out of scope

- 现有 `#logs` 的技术日志查询与 UI 布局重做。
- 将通知记录二次分组为“批次通知”“系列通知”或“聚合通知”。
- 对消息文案体系进行第二轮文案重写（沿用现有事件标签与简洁告警风格）。

## 需求（Requirements）

### MUST

- `通知记录` 页中的每张记录卡都必须代表单条真实通知，且包含 `items[]`。
- 通知记录中的机子字段必须是通知生成时的快照，不得依赖当前 `catalog_configs` 实时 join 才能渲染完整信息。
- `GET /api/notifications/records` 默认 `limit=20`，最大 `50`；使用 `ts:id` 风格 cursor，按 `created_at DESC, id DESC` 稳定排序。
- Telegram 真实通知在 `siteBaseUrl` 存在时追加：`查看通知记录：<base>/?notification=<record_id>#notifications`；为空时整行省略。
- 深链进入 `#notifications` 时，前端必须先预取目标记录并注入当前列表，再滚动定位和高亮；成功后移除 `notification` 查询参数，避免重复触发。
- 目标通知不存在或已过期时，页面仍应正常展示第一页列表，并给出明确的缺失态提示。
- 通知记录页必须使用按需加载 / 无限滚动；不得一次性拉全量。

### SHOULD

- 长列表卡片应使用 `content-visibility: auto` 等轻量渲染优化，减少离屏 DOM 开销。
- 记录卡中的机子快照尽量复用现有规格/价格展示语言，但保持只读，不出现监控开关和下单 CTA。
- API 返回中暴露每个渠道最近发送状态，便于 UI 快速显示 Telegram / Web Push 的结果概览。

## 功能与行为规格（Functional/Behavior Spec）

### Core flows

- 监控变化通知：
  - `poller` 检测到 `restock / price / config` 任一事件后，先写入 1 条通知记录，再发送 Telegram。
  - 该通知记录的 `items[]` 至少包含本次变动配置的 1 条快照。
- 生命周期通知：
  - `ops` 在 `listed / delisted` 事件发送前，先写入 1 条通知记录，再发送 Telegram / Web Push。
  - 当前链路允许每条记录只有 1 个机子项，但 API / DB 必须支持多个 `items[]`。
- Telegram 深链：
  - 用户点击 TG 中的“查看通知记录”后进入 `/?notification=<id>#notifications`。
  - Web UI 先请求 `GET /api/notifications/records/:id`，拿到目标记录后插入本地列表、去重、滚动到对应卡片、加高亮，并清理 URL 查询参数。
- 无限滚动：
  - 首屏仅拉第一页；底部 sentinel 进入视口时才继续请求下一页。
  - 若存在高亮目标且该目标已插入列表，则不得等待翻页才能定位。

### Edge cases / errors

- `notification` 查询参数为空字符串或仅空白时，按普通 `#notifications` 页面处理。
- `GET /api/notifications/records/:id` 返回 404 时，前端显示“记录不存在或已过期”，但仍继续加载第一页列表。
- 当同一记录既通过深链预取注入，又出现在分页结果中时，前端必须按 `id` 去重。
- 当 `siteBaseUrl` 不合法或为空时，通知发送链路不得因为无法拼出深链而阻塞；只是不在 Telegram 文案中附加该行。

## 接口契约（Interfaces & Contracts）

### 接口清单（Inventory）

| 接口（Name） | 类型（Kind） | 范围（Scope） | 变更（Change） | 契约文档（Contract Doc） | 负责人（Owner） | 使用方（Consumers） | 备注（Notes） |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Notification records list | HTTP API | internal | New | ./contracts/http-apis.md | backend | web | cursor 分页 + items 快照 |
| Notification record detail | HTTP API | internal | New | ./contracts/http-apis.md | backend | web | 深链预取 |
| Notification records persistence | DB | internal | New | ./contracts/db.md | backend | backend | 记录主表 + items 快照表 |
| Notification retention config | Runtime config | internal | New | ./contracts/config.md | backend | backend | 默认 30 天 / 50000 组 |
| Telegram notification deep link | Message contract | internal | Modify | ./contracts/http-apis.md | backend | Telegram users | 仅真实通知追加 |

### 契约文档（按 Kind 拆分）

- [contracts/README.md](./contracts/README.md)
- [contracts/http-apis.md](./contracts/http-apis.md)
- [contracts/db.md](./contracts/db.md)
- [contracts/config.md](./contracts/config.md)

## 验收标准（Acceptance Criteria）

- Given 监控变化 / 上新 / 下架触发真实通知
  When 后端进入发送链路
  Then 必须先写入 1 条通知记录，再发送渠道消息，且通知页看到的是通知时刻的机子快照。

- Given Telegram 文案中存在 `查看通知记录` 深链
  When 用户点击并打开 Web UI
  Then 页面直接进入 `#notifications`，目标通知被预取、自动滚动定位并高亮，随后 URL 中的 `notification` 查询参数被移除。

- Given 目标通知已不存在或过期
  When 打开深链
  Then 页面仍正常显示通知记录页与第一页列表，并提示“记录不存在或已过期”，不得白屏。

- Given 通知记录已经很多
  When 用户首次打开通知记录页
  Then 前端只请求第一页；只有滚动到底部 sentinel 时才继续拉下一页。

- Given `siteBaseUrl` 为空
  When 构建 Telegram 真实通知
  Then 文案仍可正常发送，但不会出现“查看通知记录”链接行。

## 非功能性验收 / 质量门槛（Quality Gates）

### Testing

- Backend：覆盖通知记录落库、cursor 稳定性、单条查询、保留策略、Telegram 深链拼接。
- Frontend：覆盖路由解析、深链预取注入、列表去重、高亮定位、无限滚动追加、响应式无横向溢出。

### Quality checks

- `cargo fmt`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features`
- `cd web && bun run lint`
- `cd web && bun run typecheck`
- `cd web && bun run build`
- `cd web && bun run test:storybook`

## 里程碑（Milestones）

- [x] M1: 冻结通知记录 API / DB / 深链契约并创建规格
- [x] M2: 完成后端通知记录持久化、API 与发送链路接入
- [x] M3: 完成前端通知记录页、深链定位、高亮与无限滚动
- [ ] M4: 完成本地质量门、PR、checks 与 review-loop 收敛

## 方案概述（Approach, high-level）

- 后端新增结构化通知记录表与 items 快照表，把“用户看到的通知内容”和“用于页面展示的通知快照”拆开存储。
- 通知 builder 保留现有文案风格，但新增可组合的深链输出能力；发送前统一持久化记录 ID，再回填到 Telegram 文案。
- 前端继续沿用现有 hash 路由，不引入路由库；通过 `window.location.search + history.replaceState` 管理深链查询参数。
- 通知列表使用 cursor 分页、底部 sentinel 和 `content-visibility` 降低长列表成本；目标记录走单条预取，避免为了定位而被迫翻页。

## 风险 / 开放问题 / 假设（Risks, Open Questions, Assumptions）

- 风险：通知记录如果只存主表、不存快照明细，后续 catalog 变化会让历史通知“失真”；因此本规格要求 items 快照表。
- 风险：深链预取与分页结果并发返回时，若不按 `id` 去重，容易出现重复卡片或高亮跳动。
- 需要决策的问题：None（已冻结 `1 条通知 = 1 个通知组`）。
- 假设（需主人确认）：None。

## Visual Evidence (PR)

- Storybook：`web/src/stories/pages/NotificationsView.stories.tsx` 覆盖默认态、深链态、缺失态与全断点响应式验收。
- 自动化：`cd web && bun run test:storybook` 共 15 个文件 / 51 个测试通过，其中包含 `Pages/NotificationsView`。

## 变更记录（Change log）

- 2026-03-11: 创建规格，冻结通知记录页、结构化通知存储、Telegram 深链与无限滚动契约。
- 2026-03-11: 完成通知记录落库、通知页、深链定位与本地质量门；待 PR、checks 与 review-loop 最终收口。

## 参考（References）

- `docs/specs/z9x5g-notification-copy-optimization/SPEC.md`
- `docs/specs/32dfj-partition-monitoring-new-machine-alerts/SPEC.md`
- `docs/specs/7ey9f-lazycats-cart-inventory-monitor/SPEC.md`
