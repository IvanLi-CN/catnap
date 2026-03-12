# HTTP API

## Bootstrap（GET /api/bootstrap）

- 范围（Scope）: internal
- 变更（Change）: Modify
- 鉴权（Auth）: session/header

### 变化点（Change）

- `monitoring.enabledPartitions` 继续返回 `{ countryId, regionId? }[]`，其中 `regionId = null` 表示国家监控。
- `settings.monitoringEvents` 改为：
  ```json
  {
    "partitionCatalogChangeEnabled": true,
    "regionPartitionChangeEnabled": false,
    "siteRegionChangeEnabled": true
  }
  ```
- 响应中不再返回 `siteListedEnabled`、`delistedEnabled`。

## Monitoring partition toggle（PATCH /api/monitoring/partitions）

- 范围（Scope）: internal
- 变更（Change）: Reuse
- 鉴权（Auth）: session/header

### 请求（Request）

- Body:
  ```json
  {
    "countryId": "7",
    "regionId": null,
    "enabled": true
  }
  ```
- `regionId = null` 表示“国家监控”；`regionId = "40"` 表示“可用区监控”。

### 响应（Response）

- Success（200）:
  ```json
  {
    "countryId": "7",
    "regionId": null,
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

- `monitoringEvents` 变更为：
  ```json
  {
    "partitionCatalogChangeEnabled": true,
    "regionPartitionChangeEnabled": false,
    "siteRegionChangeEnabled": true
  }
  ```

### 兼容性与迁移（Compatibility / migration）

- 历史库升级时：
  - `partitionListedEnabled -> partitionCatalogChangeEnabled`
  - `siteListedEnabled -> siteRegionChangeEnabled`
  - `regionPartitionChangeEnabled` 默认 `false`
- 旧 `delistedEnabled` 只保留数据库列兼容，不参与新版本运行时响应与写入。
