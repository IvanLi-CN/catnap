# HTTP API

## Bootstrap（GET /api/bootstrap）

- 范围（Scope）: internal
- 变更（Change）: Modify
- 鉴权（Auth）: session/header

### 变化点（Change）

- `monitoring` 新增 `enabledPartitions`。

### 响应（Response）

- Success（200）新增：
  ```json
  {
    "monitoring": {
      "enabledConfigIds": ["lc:7:40:128"],
      "enabledPartitions": [
        { "countryId": "7", "regionId": "40" },
        { "countryId": "11", "regionId": null }
      ],
      "poll": { "intervalSeconds": 60, "jitterPct": 0.1 }
    }
  }
  ```

## Monitoring partition toggle（PATCH /api/monitoring/partitions）

- 范围（Scope）: internal
- 变更（Change）: New
- 鉴权（Auth）: session/header

### 请求（Request）

- Body:
  ```json
  {
    "countryId": "7",
    "regionId": "40",
    "enabled": true
  }
  ```
- `regionId` 可为 `null`，表示 country-only 分区。

### 响应（Response）

- Success（200）:
  ```json
  {
    "countryId": "7",
    "regionId": "40",
    "enabled": true
  }
  ```

### 错误（Errors）

- `400 INVALID_ARGUMENT`: `countryId` 为空，或 `regionId` 与 `countryId` 不匹配。

## Settings（GET /api/settings, PUT /api/settings）

- 范围（Scope）: internal
- 变更（Change）: Modify
- 鉴权（Auth）: session/header

### 变化点（Change）

- `monitoringEvents` 从：
  ```json
  { "listedEnabled": true, "delistedEnabled": true }
  ```
  调整为：
  ```json
  {
    "partitionListedEnabled": true,
    "siteListedEnabled": false,
    "delistedEnabled": true
  }
  ```

### 兼容性与迁移（Compatibility / migration）

- 历史库升级时，旧 `listedEnabled` 自动映射到 `siteListedEnabled`。
- 前端与后端在同一版本切换；不提供旧字段与新字段双写响应。
