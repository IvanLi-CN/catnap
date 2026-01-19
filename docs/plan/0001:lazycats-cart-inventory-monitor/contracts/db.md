# DB（SQLite schema）

默认使用 SQLite；用于在重启后保留监控开关、设置、订阅与日志。

> 本 schema 在计划阶段先冻结口径；实现阶段可做小幅增量调整，但必须回写本文件（避免口径漂移）。

## Tables（draft）

### `users`

- `id` TEXT PRIMARY KEY
- `created_at` TEXT NOT NULL（RFC3339）

### `catalog_configs`

用于缓存最近一次抓取到的配置快照（便于 UI 快速展示）。

- `id` TEXT PRIMARY KEY（Config.id）
- `country_id` TEXT NOT NULL
- `region_id` TEXT NULL
- `name` TEXT NOT NULL
- `specs_json` TEXT NOT NULL（JSON array）
- `price_amount` REAL NOT NULL
- `price_currency` TEXT NOT NULL
- `price_period` TEXT NOT NULL
- `inventory_status` TEXT NOT NULL（`unknown|available|unavailable`）
- `inventory_quantity` INTEGER NOT NULL
- `checked_at` TEXT NOT NULL（RFC3339）
- `config_digest` TEXT NOT NULL
- `source_pid` TEXT NULL
- `source_fid` TEXT NULL
- `source_gid` TEXT NULL

### `monitoring_configs`

记录“某用户是否监控某配置”。

- `user_id` TEXT NOT NULL
- `config_id` TEXT NOT NULL
- `enabled` INTEGER NOT NULL（0/1）
- `created_at` TEXT NOT NULL
- `updated_at` TEXT NOT NULL
- PRIMARY KEY (`user_id`, `config_id`)

### `settings`

按“用户隔离”的设置（若后续决定全局共享，需要在计划中改口径并迁移）。

- `user_id` TEXT PRIMARY KEY
- `poll_interval_minutes` INTEGER NOT NULL
- `poll_jitter_pct` REAL NOT NULL
- `site_base_url` TEXT NULL
- `telegram_enabled` INTEGER NOT NULL（0/1）
- `telegram_bot_token` TEXT NULL
- `telegram_target` TEXT NULL
- `web_push_enabled` INTEGER NOT NULL（0/1）
- `created_at` TEXT NOT NULL
- `updated_at` TEXT NOT NULL

### `web_push_subscriptions`

- `id` TEXT PRIMARY KEY
- `user_id` TEXT NOT NULL
- `endpoint` TEXT NOT NULL
- `p256dh` TEXT NOT NULL
- `auth` TEXT NOT NULL
- `created_at` TEXT NOT NULL

### `event_logs`

- `id` TEXT PRIMARY KEY
- `user_id` TEXT NULL（按用户隔离；系统级日志可为空）
- `ts` TEXT NOT NULL
- `level` TEXT NOT NULL（`debug|info|warn|error`）
- `scope` TEXT NOT NULL
- `message` TEXT NOT NULL
- `meta_json` TEXT NULL

## Migration notes

- 初期不要求复杂迁移框架，但必须保证 schema 可演进（新增列优先可为空/有默认）。
- 日志与快照需要保留策略（例如按条数/天数清理）。
