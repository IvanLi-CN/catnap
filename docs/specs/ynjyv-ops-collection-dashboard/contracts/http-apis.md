# HTTP API

## Ops state snapshot（GET /api/ops/state）

- 范围（Scope）: internal
- 变更（Change）: New
- 鉴权（Auth）: same as existing internal APIs（same-origin + user identity header / session）

### 请求（Request）

- Query:
  - `range`: `24h|7d|30d`（default: `24h`）
  - `logLimit`: number（1–500, default: 200）
  - `taskLimit`: number（1–500, default: 200）

### 响应（Response）

- Success schema（JSON）:
  - `serverTime`: RFC3339 string
  - `range`: `24h|7d|30d`
  - `replayWindowSeconds`: number（固定 3600）
  - `queue`:
    - `pending`: number
    - `running`: number
    - `deduped`: number（当前队列内合并需求次数）
  - `workers`: array of
    - `workerId`: string（稳定 id，用于 UI diff）
    - `state`: `idle|running|error`
    - `task`: `{ fid: string, gid: string|null } | null`
    - `startedAt`: RFC3339 string | null
    - `lastError`: `{ ts: RFC3339, message: string } | null`
  - `tasks`: array of
    - `key`: `{ fid: string, gid: string|null }`
    - `state`: `pending|running`
    - `enqueuedAt`: RFC3339 string
    - `reasonCounts`: object（key 为原因类型，value 为计数）
    - `lastRun`: `{ endedAt: RFC3339, ok: boolean } | null`
  - `stats`:
    - `collection`: `{ total: number, success: number, failure: number, successRatePct: number }`
    - `notify`: `{ telegram?: {...}, webPush?: {...} }`（每渠道同样结构）
  - `sparks`（用于 KPI 卡片 sparkline）:
    - `bucketSeconds`: number（bucket 粒度；24h=3600，7d/30d=86400）
    - `volume`: number[]（每 bucket 的任务量）
    - `collectionSuccessRatePct`: number[]（每 bucket 的采集成功率百分比）
    - `notifyTelegramSuccessRatePct`: number[]（每 bucket 的 Telegram 成功率百分比）
    - `notifyWebPushSuccessRatePct`: number[]（每 bucket 的 Web Push 成功率百分比）
  - `logTail`: array of（最近 N 条）
    - `eventId`: number
    - `ts`: RFC3339 string
    - `level`: `debug|info|warn|error`
    - `scope`: string
    - `message`: string
    - `meta`: object | null
- Error schema:
  - `{ "error": { "code": string, "message": string } }`

### 错误（Errors）

- `400 invalid_argument`: 参数非法（range/limit）
- `401/403`: 鉴权失败（沿用现有策略）

## Ops stream（GET /api/ops/stream）

- 范围（Scope）: internal
- 变更（Change）: New
- 鉴权（Auth）: same as existing internal APIs（same-origin + user identity header / session）

### 请求（Request）

- Query:
  - `range`: `24h|7d|30d`（default: `24h`；影响 `ops.metrics` 事件口径）
- Headers:
  - `Last-Event-ID`: string（可选；必须为十进制整数；用于断线续传）

### 响应（Response）

- `200 text/event-stream`
  - 事件格式与语义见 `./events.md`

### 错误（Errors）

- `400 invalid_argument`: `range` 非法
- `401/403`: 鉴权失败（沿用现有策略）

（说明：`Last-Event-ID` 非法或过旧不返回 HTTP 错误；服务端应建立 SSE 连接并发送 `ops.reset`，由客户端重置为“重新拉 snapshot + 无 Last-Event-ID 重连”。）
