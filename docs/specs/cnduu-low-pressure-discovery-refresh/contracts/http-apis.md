# HTTP API Contract

本计划不新增外部 API；所有变更均为 internal、non-breaking 语义补充或文案调整。

## GET /api/products

- 变更类型：Modify
- 响应 shape：不变
- 行为约束：
  - 前端继续按现有 10–30 秒后台刷新策略轮询。
  - 当后台 discovery / poller 已完成 apply diff 后，`configs[]` 中的 lifecycle 与 inventory 变化必须在下一次 products 轮询中可见。

## GET /api/monitoring

- 变更类型：Modify
- 响应 shape：不变（仍返回 `items`, `fetchedAt`, `recentListed24h`）
- 行为约束：
  - 监控页除进入页面时刷新外，必须增加后台轮询，目标是在 DB 中 `recentListed24h` 更新后 `<=30s` 反映到 UI。
  - `recentListed24h` 继续表示最近 24 小时发生 listed（含 relisted）的配置列表。

## POST /api/catalog/refresh

- 变更类型：Modify
- 响应 shape：不变
- 行为约束：
  - `manual_refresh` 继续表示“推进已知 URL 子任务”的手动刷新。
  - 命中 `300s` freshness window 的 `url_key` 必须走 cache hit，不得强制 bypass 上游。
  - cache hit 与真实 fetch 都必须推进 job 进度与 SSE 状态。

## GET /api/catalog/refresh/events

- 变更类型：Modify
- 响应 shape：兼容现有 `catalog.refresh` 事件
- 行为约束：
  - `current.action` 继续使用 `fetch | cache`。
  - 当 `manual_refresh` 命中 cache hit 时，前端必须能区分该状态，而不是误判为真实抓取。

## GET /api/settings / PUT /api/settings

- 变更类型：Modify
- 字段 shape：保留 `settings.catalogRefresh.autoIntervalHours`
- 语义调整：
  - 该字段改为只读的有效值，固定返回 `1`（小时）。
  - `PUT /api/settings` 继续接受该字段以保持兼容，但服务端忽略用户传入值。
  - 本计划不新增终端用户可配置的 discovery 频率字段。

## GET /api/ops/state

- 变更类型：Modify
- 响应补充（允许在现有对象中增量扩展字段）：
  - queue aging：最老待执行任务等待时长；
  - discovery/cache-hit 可观测性：能按 reason 区分 `discovery_due`、`poller_due`、`manual_refresh`、`topology_refresh`；
  - topology freshness：最近一次 topology refresh 成功时间。
- 兼容要求：
  - 旧字段保留；新增字段只做增量追加，不破坏现有页面。
