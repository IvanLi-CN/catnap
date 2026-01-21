# HTTP API

本计划新增“库存历史（近 24h）”查询接口，供 UI 在配置卡片上绘制每分钟走势。

通用约定（与 #0001 对齐）：

- Base path: `/api`
- 响应为 JSON
- 鉴权：required（不暴露具体识别方式）
- Error shape（统一）:
  - `{"error":{"code":"...","message":"..."}}`

## Inventory history（POST /api/inventory/history）

- 范围（Scope）: internal
- 变更（Change）: New
- 鉴权（Auth）: required

### 请求（Request）

- Body:
  - `configIds`: `string[]`（min 1）

示例：

- Request（请求）:
  - `{"configIds":["cfg_123","cfg_456"]}`

### 响应（Response）

- Success:
  - `200 OK`
  - JSON:
    - `window`:
      - `from`: RFC3339 timestamp（minute-aligned）
      - `to`: RFC3339 timestamp（minute-aligned）
    - `series`: `InventoryHistorySeries[]`

`InventoryHistorySeries`:

- `configId`: `string`
- `points`: `InventoryHistoryPoint[]`（sparse points）
  - order: oldest → newest（按时间排序）
  - 仅返回窗口内存在记录的 minute bucket

`InventoryHistoryPoint`:

- `tsMinute`: RFC3339 timestamp（minute-aligned）
- `quantity`: `number`（integer, >=0；raw value，UI 自行将 >10 显示为 10+）

说明（时间轴口径）：

- UI 绘制时必须按 `tsMinute` 的真实时间比例映射 X 轴，不能把 “points 数量” 当作均匀采样来平分横坐标。
- 本接口窗口固定为“滚动 24 小时（rolling 24h）”。
- 若后续需要 dense（每分钟固定 1440 点）或可变窗口：应另开变更并冻结补齐规则与上限。

- Error:
  - `400`: `{"error":{"code":"INVALID_ARGUMENT","message":"..."}}`（如 `configIds` 为空/过多）
  - `401`: `{"error":{"code":"UNAUTHORIZED","message":"Unauthorized"}}`
  - `403`: `{"error":{"code":"FORBIDDEN","message":"Forbidden"}}`

### 示例（Examples）

- Response（响应）:
  - `200`:
    - `{"window":{"from":"2026-01-20T12:30:00Z","to":"2026-01-20T12:34:00Z"},"series":[{"configId":"cfg_123","points":[{"tsMinute":"2026-01-20T12:31:00Z","quantity":0},{"tsMinute":"2026-01-20T12:33:00Z","quantity":1}]},{"configId":"cfg_456","points":[]}]}` 

### 兼容性与迁移（Compatibility / migration）

- 新增 endpoint，不影响既有调用方。
- 若后续决定把历史走势合并进 `/api/bootstrap` 或 `/api/monitoring`：需要单独计划并更新 #0001 契约（避免负载与 payload 暴增带来回归风险）。
