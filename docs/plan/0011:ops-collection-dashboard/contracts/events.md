# 事件（Events）

本文件定义 `GET /api/ops/stream` 的 SSE 事件类型与载荷。所有事件：

- 必须包含 `id`（单调递增的十进制整数，作为 `Last-Event-ID` 的对齐点）
- 投递语义：at-least-once、有序（按 `id`），客户端需按 `id` 去重
- `data` 为 JSON 对象（UTF-8）

## ops.hello

- 范围（Scope）: internal
- 变更（Change）: New
- 生产者（Producer）: backend
- 消费者（Consumers）: web
- 投递语义（Delivery semantics）: at-least-once, ordered by `id`, retry by reconnect

### 载荷（Payload）

- Schema:
  - `serverTime`: RFC3339 string
  - `range`: `24h|7d|30d`
  - `replayWindowSeconds`: number（固定 3600）

## ops.reset

用于告知客户端“无法进行续传回放”，客户端应执行：重新拉取 `/api/ops/state` 并以 `Last-Event-ID` 为空重连。

- 范围（Scope）: internal
- 变更（Change）: New
- 生产者（Producer）: backend
- 消费者（Consumers）: web

### 载荷（Payload）

- Schema:
  - `serverTime`: RFC3339 string
  - `reason`: `stale_last_event_id|invalid_last_event_id|server_restart|schema_changed`
  - `details`: string（可选，人类可读）

## ops.queue

队列状态更新（可以是增量或快照）。

- 范围（Scope）: internal
- 变更（Change）: New
- 生产者（Producer）: backend
- 消费者（Consumers）: web

### 载荷（Payload）

- Schema:
  - `queue`: `{ pending: number, running: number, deduped: number }`
  - `tasksDelta`: array（可选；变化项）

## ops.worker

worker 状态更新（可以是增量或快照）。

- 范围（Scope）: internal
- 变更（Change）: New
- 生产者（Producer）: backend
- 消费者（Consumers）: web

### 载荷（Payload）

- Schema:
  - `workers`: array（与 snapshot schema 对齐）

## ops.task

任务生命周期事件（enqueue/start/end/error）。

- 范围（Scope）: internal
- 变更（Change）: New
- 生产者（Producer）: backend
- 消费者（Consumers）: web

### 载荷（Payload）

- Schema:
  - `phase`: `enqueued|started|finished`
  - `key`: `{ fid: string, gid: string|null }`
  - `reasonCounts`: object（仅在 `enqueued` 或合并时包含）
  - `run`（在 `started/finished` 包含）:
    - `runId`: number
    - `startedAt`: RFC3339 string
    - `endedAt`: RFC3339 string | null
    - `ok`: boolean | null
    - `fetch`: `{ url: string, httpStatus: number, bytes: number, elapsedMs: number } | null`
    - `parse`: `{ ok: boolean, producedConfigs: number, elapsedMs: number } | null`
    - `error`: `{ code: string, message: string } | null

## ops.notify

推送发送结果（每渠道一条）。

- 范围（Scope）: internal
- 变更（Change）: New
- 生产者（Producer）: backend
- 消费者（Consumers）: web

### 载荷（Payload）

- Schema:
  - `runId`: number（关联任务 run）
  - `channel`: `telegram|webPush`
  - `result`: `success|error|skipped`
  - `message`: string（可选；例如“已发送到 xxx”/“缺少 token”/“http 500”）

## ops.log

聚合日志（用于 UI tail 展示），必须覆盖成果与推送触发情况。

- 范围（Scope）: internal
- 变更（Change）: New
- 生产者（Producer）: backend
- 消费者（Consumers）: web

### 载荷（Payload）

- Schema:
  - `ts`: RFC3339 string
  - `level`: `debug|info|warn|error`
  - `scope`: string
  - `message`: string
  - `meta`: object | null

