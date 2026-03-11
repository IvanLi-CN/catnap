# 数据库（DB）

## 用户监控范围（monitoring_partitions）

- 范围（Scope）: internal
- 变更（Change）: Reuse
- 影响表（Affected tables）: `monitoring_partitions`

### 语义（Semantics）

- `region_id IS NULL`：地区监控 scope。
- `region_id IS NOT NULL`：可用区监控 scope。
- 运行时不得再把 `region_id IS NULL` 解释为“默认可用区监控”。

### Migration notes（迁移说明）

- 不新增新表；继续复用 `monitoring_partitions`。
- 历史 `region_id IS NULL` 记录在新语义下统一解释为地区监控。

## Settings monitoring events

- 范围（Scope）: internal
- 变更（Change）: Modify
- 影响表（Affected tables）: `settings`

### Schema delta（结构变更）

- 运行时字段改为：
  - `monitoring_events_partition_catalog_change_enabled INTEGER NOT NULL DEFAULT 0`
  - `monitoring_events_region_partition_change_enabled INTEGER NOT NULL DEFAULT 0`
  - `monitoring_events_site_region_change_enabled INTEGER NOT NULL DEFAULT 0`
- 旧列：
  - `monitoring_events_partition_listed_enabled`
  - `monitoring_events_site_listed_enabled`
  - `monitoring_events_delisted_enabled`
  仅保留迁移兼容，不再作为新版本运行时读写来源。

### Migration notes（迁移说明）

- 启动期自动补列。
- 若新列不存在，则执行一次回填：
  - `partition_catalog_change_enabled = partition_listed_enabled`
  - `site_region_change_enabled = site_listed_enabled`
  - `region_partition_change_enabled = 0`
- 旧 `delisted_enabled` 不自动扩散映射到任何新层级开关。

## Topology change routing

- 范围（Scope）: internal
- 变更（Change）: New
- 影响模块（Affected modules）: `poller`, `ops`, `notification_content`

### 规则（Rules）

- 新地区 / 新可用区：由 topology probe/refresh 的 snapshot diff 产出。
- 地区删除 / 可用区删除：仅由正式 `topology_refresh` 的前后 diff 产出。
- 收件人：
  - 新地区 / 地区删除 -> `settings.site_region_change_enabled = 1`
  - 新可用区 / 可用区删除 -> `monitoring_partitions(country_id, NULL)` + `settings.region_partition_change_enabled = 1`
  - 套餐新增 / 套餐删除 -> `monitoring_partitions(country_id, region_id)` + `settings.partition_catalog_change_enabled = 1`
