# DB（#xm4p2）

## New tables

### `notification_records`

- `id` TEXT PRIMARY KEY
- `user_id` TEXT NOT NULL
- `created_at` TEXT NOT NULL
- `kind` TEXT NOT NULL
  - 监控变化：`monitoring.<event[+event...]>`
  - 生命周期：`catalog.partition_listed | catalog.site_listed | catalog.delisted`
- `title` TEXT NOT NULL
- `summary` TEXT NOT NULL
- `partition_label` TEXT NULL
- `telegram_status` TEXT NOT NULL DEFAULT `not_sent`
- `web_push_status` TEXT NOT NULL DEFAULT `not_sent`

索引：

- `(user_id, created_at DESC, id DESC)` 用于用户隔离后的稳定分页

### `notification_record_items`

- `id` TEXT PRIMARY KEY
- `record_id` TEXT NOT NULL
- `position` INTEGER NOT NULL
- `config_id` TEXT NULL
- `name` TEXT NOT NULL
- `country_name` TEXT NOT NULL
- `region_name` TEXT NULL
- `specs_json` TEXT NOT NULL
- `price_amount` REAL NOT NULL
- `price_currency` TEXT NOT NULL
- `price_period` TEXT NOT NULL
- `inventory_status` TEXT NOT NULL
- `inventory_quantity` INTEGER NOT NULL
- `checked_at` TEXT NOT NULL
- `lifecycle_state` TEXT NOT NULL
- `lifecycle_listed_at` TEXT NOT NULL
- `lifecycle_delisted_at` TEXT NULL

索引：

- `(record_id, position ASC)` 用于按通知生成顺序回放 `items[]`

## Snapshot rules

- `notification_record_items` 保存通知生成时的名称、分区名称、规格、价格、库存、生命周期快照。
- 页面渲染不依赖重新 join 当前 `catalog_configs`；即使原配置被下架或目录更新，历史通知仍可独立渲染。
- `partitionLabel` 在读取时由 `country_name + region_name` 组合生成；record 级别的 `partition_label` 用于组头部摘要显示。

## Retention

- 新增独立保留策略：`CATNAP_NOTIFICATION_RETENTION_DAYS`（默认 `30`）与 `CATNAP_NOTIFICATION_RETENTION_MAX_ROWS`（默认 `50000`）。
- 清理顺序：
  1. 先删除超出保留天数的 `notification_records`
  2. 再按 `created_at DESC, id DESC` 仅保留最新 `max_rows` 条主记录
  3. 最后删除失去主记录引用的 `notification_record_items`
