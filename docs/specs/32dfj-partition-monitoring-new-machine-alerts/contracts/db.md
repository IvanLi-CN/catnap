# 数据库（DB）

## 用户分区订阅（monitoring_partitions）

- 范围（Scope）: internal
- 变更（Change）: New
- 影响表（Affected tables）: `monitoring_partitions`

### Schema delta（结构变更）

- 新表：
  - `user_id TEXT NOT NULL`
  - `partition_key TEXT NOT NULL`（`countryId::regionId?` 归一化，`regionId=null` 时为空串）
  - `country_id TEXT NOT NULL`
  - `region_id TEXT NULL`
  - `enabled INTEGER NOT NULL`
  - `created_at TEXT NOT NULL`
  - `updated_at TEXT NOT NULL`
  - `PRIMARY KEY (user_id, partition_key)`
- 索引：
  - `(user_id, enabled, updated_at DESC)`
  - `(country_id, region_id, enabled)`

### Migration notes（迁移说明）

- 新表通过启动期 `CREATE TABLE IF NOT EXISTS` 创建；无需单独 migration 文件。
- 禁止把分区订阅与配置卡片监控共用表，避免语义污染。

## Settings listed 迁移

- 范围（Scope）: internal
- 变更（Change）: Modify
- 影响表（Affected tables）: `settings`

### Schema delta（结构变更）

- 新增列：
  - `monitoring_events_partition_listed_enabled INTEGER NOT NULL DEFAULT 0`
  - `monitoring_events_site_listed_enabled INTEGER NOT NULL DEFAULT 0`
- 保留旧列 `monitoring_events_listed_enabled` 仅用于老库回填，不再作为运行时读写来源。

### Migration notes（迁移说明）

- 若启动时检测到新 `site` 列尚不存在，则在补列后立刻执行一次：
  - `monitoring_events_site_listed_enabled = monitoring_events_listed_enabled`
- `monitoring_events_partition_listed_enabled` 默认为 `0`，不从旧列继承。
- 新版本后续仅读写 `partition/site/delisted` 三个运行时字段。
