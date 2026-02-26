# HTTP API

本计划的接口均为 internal；鉴权沿用现有“用户 id header + same-origin”策略。

## 立即全量刷新（POST /api/catalog/refresh）

- 范围（Scope）: internal
- 变更（Change）: New
- 鉴权（Auth）: session/header（与现有 API 一致）

### 请求（Request）

- Headers: `X-User-Id: <string>`（现有约定）
- Body: `null`

### 响应（Response）

- Success（200）:
  ```json
  {
    "jobId": "uuid",
    "state": "idle|running|success|error",
    "trigger": "manual|auto",
    "done": 0,
    "total": 0,
    "message": null,
    "startedAt": "2026-01-23T00:00:00Z",
    "updatedAt": "2026-01-23T00:00:00Z"
  }
  ```
- Error:
  - 429 `RATE_LIMITED`（手动触发限流）
  - 500 `INTERNAL`

### 兼容性与迁移（Compatibility / migration）

- “立即刷新”按钮改用该端点，确保语义为全量刷新。
- 当前工程已存在旧接口（`POST /api/refresh` + `GET /api/refresh/status`，前端轮询使用）。实现阶段建议：
  - 新前端切换到本计划的 `/api/catalog/refresh` + SSE；
  - 旧接口保留为 **deprecated shim**（可选）：仍可触发/读取同一全量刷新 job 的状态，便于平滑升级与排障。

## 刷新状态 SSE（GET /api/catalog/refresh/events）

- 范围（Scope）: internal
- 变更（Change）: New
- 鉴权（Auth）: session/header（与现有 API 一致）

### 请求（Request）

- Headers:
  - `Accept: text/event-stream`
  - `Cache-Control: no-cache`

### 响应（Response）

- Success（200）: `text/event-stream`
  - 事件名：`catalog.refresh`
  - `data:` 为 JSON（示例见 `contracts/events.md`）

### 错误（Errors）

- 401/403（鉴权失败）
- 500（内部错误）

## Bootstrap（GET /api/bootstrap）

- 范围（Scope）: internal
- 变更（Change）: Modify
- 鉴权（Auth）: session/header（与现有 API 一致）

### 变化点（Change）

- `settings` 增加字段（自动全量刷新间隔、上架/下架监控开关）。
- `catalog.configs[]`（或等价返回的 configs）增加 lifecycle 字段用于 UI 显示“下架”。

### 响应（Response）新增字段（示意）

- `settings.catalogRefresh.autoIntervalHours: number | null`
- `settings.monitoringEvents.listedEnabled: boolean`
- `settings.monitoringEvents.delistedEnabled: boolean`
- `catalog.configs[].lifecycle.state: "active" | "delisted"`
- `catalog.configs[].lifecycle.listedAt: string`（RFC3339）
- `catalog.configs[].lifecycle.delistedAt?: string | null`（RFC3339；active 时为 null/缺省）

## Products（GET /api/products）

- 范围（Scope）: internal
- 变更（Change）: Modify
- 鉴权（Auth）: session/header（与现有 API 一致）

### 变化点（Change）

- `configs[]` 增加 lifecycle 字段（与 `/api/bootstrap` 保持一致）。

### 响应（Response）新增字段（示意）

- `configs[].lifecycle.state: "active" | "delisted"`
- `configs[].lifecycle.listedAt: string`（RFC3339）
- `configs[].lifecycle.delistedAt?: string | null`（RFC3339）

## Monitoring（GET /api/monitoring）

- 范围（Scope）: internal
- 变更（Change）: Modify
- 鉴权（Auth）: session/header（与现有 API 一致）

### 变化点（Change）

- 响应增加字段 `recentListed24h`：最近 24 小时发生“上架（listed，含重新上架）”的配置列表（用于监控页顶部展示）。

### 响应（Response）新增字段（示意）

- Success（200）:
  ```json
  {
    "items": [],
    "fetchedAt": "2026-01-23T00:00:00Z",
    "recentListed24h": []
  }
  ```

## Settings（GET /api/settings, PUT /api/settings）

- 范围（Scope）: internal
- 变更（Change）: Modify
- 鉴权（Auth）: session/header（与现有 API 一致）

### 变化点（Change）

- `SettingsView` 增加：
  - `catalogRefresh.autoIntervalHours: number | null`（null=关闭）
  - `monitoringEvents.listedEnabled: boolean`
  - `monitoringEvents.delistedEnabled: boolean`
  - 多用户语义：系统取所有用户中“启用的 `autoIntervalHours`”的最小值，作为全局自动全量刷新间隔

### 响应（Response）新增字段（示意）

- Success（200）:
  ```json
  {
    "poll": { "intervalMinutes": 1, "jitterPct": 0.2 },
    "siteBaseUrl": "https://catnap.ivanli.cc",
    "catalogRefresh": { "autoIntervalHours": 6 },
    "monitoringEvents": { "listedEnabled": true, "delistedEnabled": true },
    "notifications": {
      "telegram": { "enabled": true, "configured": true, "target": "-100xxxx" },
      "webPush": { "enabled": false, "vapidPublicKey": "..." }
    }
  }
  ```
