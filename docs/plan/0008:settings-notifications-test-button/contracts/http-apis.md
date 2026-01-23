# HTTP APIs（#0008）

## Telegram：发送测试消息（POST /api/notifications/telegram/test）

- 范围（Scope）: internal
- 变更（Change）: New
- 鉴权（Auth）: 复用现有 `/api/*` 的鉴权与 same-origin 策略（反向代理注入用户 header；浏览器同源请求）。

### 请求（Request）

- Headers:
  - `Content-Type: application/json`
  - 其余鉴权/同源相关 header 与现有 API 一致（例如用户 header、`Origin`）。
- Body（JSON）:
  - `botToken`: `string | null`（可选；若提供则仅用于本次发送；不保存）
  - `target`: `string | null`（可选；若提供则仅用于本次发送；不保存）
  - `text`: `string | null`（可选；为 `null` 时使用默认测试消息文本）

#### 校验（Validation）

- `botToken` 的最终取值为 `req.botToken ?? saved.botToken`；必须非空，否则返回 `400 INVALID_ARGUMENT`。
- `target` 的最终取值为 `req.target ?? saved.target`；必须非空，否则返回 `400 INVALID_ARGUMENT`。
- `text` 允许为空；为空时由服务端生成默认测试消息（包含时间戳即可，不包含敏感信息）。

#### 重要语义（No persistence）

- 本 endpoint **不会**修改/保存任何 settings 字段；`botToken`/`target` 若被提供，仅用于本次发送。

### 响应（Response）

- Success (`200`):
  - `{"ok": true}`
- Error (`400` / `5xx`):
  - `{"error":{"code":"INVALID_ARGUMENT","message":"Invalid argument"}}`
  - 或：`{"error":{"code":"INTERNAL","message":"Internal error"}}`

### 错误（Errors）

- `400 / INVALID_ARGUMENT`: 缺少可用的 `botToken`/`target`（retryable: no）
- `5xx / INTERNAL`: Telegram 上游请求失败（非 2xx / 网络错误等）（retryable: yes）

### 示例（Examples）

- Request:
  - `POST /api/notifications/telegram/test`
  - Body: `{"botToken":null,"target":"-1001234567890","text":null}`
- Response（success）:
  - `{"ok":true}`

### 兼容性与迁移（Compatibility / migration）

- 新增 endpoint，不影响现有调用方；UI 在支持测试按钮后才会调用。

---

## Web Push：发送测试 Push（POST /api/notifications/web-push/test）

- 范围（Scope）: internal
- 变更（Change）: New
- 鉴权（Auth）: 复用现有 `/api/*` 的鉴权与 same-origin 策略（反向代理注入用户 header；浏览器同源请求）。

### 请求（Request）

- Headers:
  - `Content-Type: application/json`
- Body（JSON）:
  - `subscription`: `WebPushSubscription`（必填；不保存）
    - `endpoint`: `string`
    - `keys`: `{ p256dh: string, auth: string }`
  - `title`: `string | null`（可选；默认 `catnap`）
  - `body`: `string | null`（可选；默认空）
  - `url`: `string | null`（可选；默认 `/`；用于 notification click 跳转）

### 响应（Response）

- Success (`200`):
  - `{"ok": true}`
- Error (`400` / `5xx`):
  - `{"error":{"code":"INVALID_ARGUMENT","message":"..."}}`
  - `{"error":{"code":"INTERNAL","message":"..."}}`

### 错误（Errors）

- `400 / INVALID_ARGUMENT`: subscription 不完整（endpoint/keys 缺失等）（retryable: no）
- `5xx / INTERNAL`: push service 返回非 2xx、网络错误、或服务端缺少 VAPID private key/subject（retryable: yes/no 视具体错误）

### 示例（Examples）

- Request:
  - `POST /api/notifications/web-push/test`
  - Body:
    - `{"subscription":{"endpoint":"https://push.example.com/..","keys":{"p256dh":"..","auth":".."}},"title":"catnap","body":"test","url":"/settings"}`
- Response（success）:
  - `{"ok":true}`

### 兼容性与迁移（Compatibility / migration）

- 新增 endpoint，不影响现有调用方；subscription 的存储仍使用既有 `POST /api/notifications/web-push/subscriptions`。
