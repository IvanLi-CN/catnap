# HTTP API

每个 endpoint 一个小节；保持短小但可实现、可测试。

通用约定：

- Base path: `/api`
- 响应为 JSON（除非明确说明为 HTML）。
- 鉴权：所有接口均要求“已识别用户”。若缺少用户信息，返回 `401`，且错误信息不暴露鉴权细节。
- 同源：不提供 CORS。对于带 `Origin` 的请求，若非同源则返回 `403`（或等价拒绝策略）。
- Error shape（统一）:
  - `{"error":{"code":"...","message":"..."}}`

## Bootstrap（GET /api/bootstrap）

- 范围（Scope）: internal
- 变更（Change）: New
- 鉴权（Auth）: required（不暴露具体识别方式）

### 请求（Request）

- Headers: none
- Query: none
- Body: none

### 响应（Response）

- Success:
  - `200 OK`
  - JSON:
    - `user`: `{ id: string, displayName?: string }`
    - `catalog`:
      - `countries`: `Country[]`
      - `regions`: `Region[]`
      - `configs`: `Config[]`
      - `fetchedAt`: RFC3339 timestamp
      - `source`: `{ url: string }`
    - `monitoring`:
      - `enabledConfigIds`: `string[]`
      - `poll`: `{ intervalSeconds: number, jitterPct: number }`
    - `settings`: `SettingsView`

- Error:
  - `401`: `{"error":{"code":"UNAUTHORIZED","message":"Unauthorized"}}`
  - `403`: `{"error":{"code":"FORBIDDEN","message":"Forbidden"}}`

### 示例（Examples）

- Response（响应）:
  - `200`:
    - `{"user":{"id":"u_123"},"catalog":{"countries":[],"regions":[],"configs":[],"fetchedAt":"2026-01-18T00:00:00Z","source":{"url":"https://lazycats.vip/cart"}},"monitoring":{"enabledConfigIds":[],"poll":{"intervalSeconds":60,"jitterPct":0.1}},"settings":{"notifications":{"telegram":{"enabled":false},"webPush":{"enabled":false}},"siteBaseUrl":null}}`

## Products（GET /api/products）

- 范围（Scope）: internal
- 变更（Change）: New
- 鉴权（Auth）: required

### 请求（Request）

- Query (optional):
  - `countryId`: string
  - `regionId`: string

### 响应（Response）

- Success:
  - `200 OK`
  - JSON:
    - `configs`: `Config[]`
    - `fetchedAt`: RFC3339 timestamp

## Monitoring list（GET /api/monitoring）

- 范围（Scope）: internal
- 变更（Change）: New
- 鉴权（Auth）: required

### 响应（Response）

- Success:
  - `200 OK`
  - JSON:
    - `items`: `Config[]`（仅包含已启用监控的配置）
    - `fetchedAt`: RFC3339 timestamp

说明：

- 监控页在前端按“可用区（region）一行”做分组；每行内部以网格展示配置；支持折叠（默认展开）。

## Monitoring toggle（PATCH /api/monitoring/configs/{configId}）

- 范围（Scope）: internal
- 变更（Change）: New
- 鉴权（Auth）: required

### 请求（Request）

- Path params:
  - `configId`: string
- Body:
  - `{"enabled": true}`

### 响应（Response）

- Success:
  - `200 OK`
  - JSON:
    - `{"configId":"...","enabled":true}`
- Error:
  - `400`: `{"error":{"code":"INVALID_ARGUMENT","message":"..."}}`（例如该配置不支持监控）

## Settings（GET /api/settings）

- 范围（Scope）: internal
- 变更（Change）: New
- 鉴权（Auth）: required

### 响应（Response）

- Success:
  - `200 OK`
  - JSON: `SettingsView`

`SettingsView`（UI 读取用）：

- `poll`: `{ intervalMinutes: number, jitterPct: number }`
- `siteBaseUrl`: `string | null`
- `notifications`:
  - `telegram`: `{ enabled: boolean, configured: boolean, target?: string }`（敏感字段不回显）
  - `webPush`: `{ enabled: boolean, vapidPublicKey?: string }`

## Settings（PUT /api/settings）

- 范围（Scope）: internal
- 变更（Change）: New
- 鉴权（Auth）: required

### 请求（Request）

- Body:
  - `poll`:
    - `intervalMinutes`: number（>= 1）
    - `jitterPct`: number（0..1）
  - `siteBaseUrl`: string | null
  - `notifications`:
    - `telegram`:
      - `enabled`: boolean
      - `botToken`: string | null
      - `target`: string | null（chat id 或频道）
    - `webPush`:
      - `enabled`: boolean

### 响应（Response）

- Success:
  - `200 OK` + `SettingsView`
- Error:
  - `400`: `{"error":{"code":"INVALID_ARGUMENT","message":"..."}}`

## Logs（GET /api/logs）

- 范围（Scope）: internal
- 变更（Change）: New
- 鉴权（Auth）: required

### 请求（Request）

- Query (optional):
  - `level`: `debug|info|warn|error`
  - `cursor`: string（opaque）
  - `limit`: number（default 50, max 200）

### 响应（Response）

- Success:
  - `200 OK`
  - JSON:
    - `items`: `LogEntry[]`
    - `nextCursor`: string | null

## Web Push: subscribe（POST /api/notifications/web-push/subscriptions）

- 范围（Scope）: internal
- 变更（Change）: New
- 鉴权（Auth）: required

### 请求（Request）

- Body:
  - `subscription`:
    - `endpoint`: string
    - `keys`:
      - `p256dh`: string
      - `auth`: string

### 响应（Response）

- Success:
  - `200 OK`
  - JSON:
    - `{"subscriptionId":"..."}`

---

## Types（shared）

`Country`:

- `id`: string
- `name`: string

`Region`:

- `id`: string
- `countryId`: string
- `name`: string
- `locationName?`: string

`Config`:

- `id`: string
- `countryId`: string
- `regionId`: string | null
- `name`: string
- `specs`: `{ key: string, value: string }[]`
- `price`:
  - `amount`: number
  - `currency`: string（例如 `CNY`）
  - `period`: string（例如 `month`）
- `inventory`:
  - `status`: `unknown|available|unavailable`（可由 `quantity` 推导：`0 => unavailable`, `>0 => available`）
  - `quantity`: number（integer, >= 0）
  - `checkedAt`: RFC3339 timestamp
  - Note: 当 `monitorSupported=false` 时，服务端返回 `status=available` 且 `quantity=1`（占位值，用于兼容统一结构；客户端展示“有货”即可）。
- `digest`: string（用于检测“配置变化”的稳定摘要）
- `monitorSupported`: boolean（是否支持监控；例如 `countryId=2` 的云服务器配置不支持监控）
- `monitorEnabled`: boolean

`LogEntry`:

- `id`: string
- `ts`: RFC3339 timestamp
- `level`: `debug|info|warn|error`
- `scope`: string
- `message`: string
- `meta?`: object
