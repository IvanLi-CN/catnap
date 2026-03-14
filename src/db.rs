use crate::config::RuntimeConfig;
use crate::defaults::FIXED_CATALOG_TOPOLOGY_REFRESH_INTERVAL_HOURS;
use crate::models::*;
use sqlx::{Row, SqlitePool};
use time::{
    format_description::{well_known::Rfc3339, FormatItem},
    macros::format_description,
    OffsetDateTime, UtcOffset,
};
use uuid::Uuid;

const CANONICAL_RFC3339: &[FormatItem<'static>] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:9]Z");
const NOTIFICATION_RECORD_CURSOR_MAX_TS: &str = "9999-12-31T23:59:59.999999999Z";

pub fn normalize_telegram_targets<I, S>(targets: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut out = Vec::new();
    for raw in targets {
        let target = raw.as_ref().trim();
        if target.is_empty() {
            continue;
        }
        if out.iter().any(|seen| seen == target) {
            continue;
        }
        out.push(target.to_string());
    }
    out
}

fn parse_telegram_targets_json(raw: Option<&str>) -> Vec<String> {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Vec::new();
    };
    let parsed = serde_json::from_str::<Vec<String>>(raw).unwrap_or_default();
    normalize_telegram_targets(parsed)
}

pub fn telegram_targets_from_storage(
    telegram_targets_json: Option<&str>,
    legacy_target: Option<&str>,
) -> Vec<String> {
    let parsed = parse_telegram_targets_json(telegram_targets_json);
    if !parsed.is_empty() {
        return parsed;
    }
    normalize_telegram_targets(legacy_target)
}

pub fn aggregate_telegram_status(
    attempted: bool,
    deliveries: &[NotificationRecordDeliveryView],
) -> String {
    if !attempted {
        return "skipped".to_string();
    }
    if deliveries.is_empty() {
        return "skipped".to_string();
    }

    let success = deliveries.iter().filter(|item| item.status == "success").count();
    let error = deliveries.iter().filter(|item| item.status == "error").count();

    if success == deliveries.len() {
        "success".to_string()
    } else if error == deliveries.len() {
        "error".to_string()
    } else if success > 0 && error > 0 {
        "partial_success".to_string()
    } else if deliveries.iter().any(|item| item.status == "pending") {
        "pending".to_string()
    } else if deliveries.iter().all(|item| item.status == "skipped") {
        "skipped".to_string()
    } else {
        "error".to_string()
    }
}

#[derive(Debug, Clone)]
pub struct SettingsRow {
    pub poll_interval_minutes: i64,
    pub poll_jitter_pct: f64,
    pub site_base_url: Option<String>,

    pub catalog_refresh_auto_interval_hours: Option<i64>,
    pub monitoring_events_partition_catalog_change_enabled: bool,
    pub monitoring_events_region_partition_change_enabled: bool,
    pub monitoring_events_site_region_change_enabled: bool,

    pub telegram_enabled: bool,
    pub telegram_bot_token: Option<String>,
    pub telegram_target: Option<String>,
    pub telegram_targets: Vec<String>,

    pub web_push_enabled: bool,

    pub created_at: String,
    pub updated_at: String,
}

impl SettingsRow {
    pub fn to_view(&self, vapid_public_key: Option<String>) -> SettingsView {
        SettingsView {
            poll: SettingsPollView {
                interval_minutes: self.poll_interval_minutes,
                jitter_pct: self.poll_jitter_pct,
            },
            site_base_url: self.site_base_url.clone(),
            catalog_refresh: SettingsCatalogRefreshView {
                auto_interval_hours: Some(FIXED_CATALOG_TOPOLOGY_REFRESH_INTERVAL_HOURS),
            },
            monitoring_events: SettingsMonitoringEventsView {
                partition_catalog_change_enabled: self
                    .monitoring_events_partition_catalog_change_enabled,
                region_partition_change_enabled: self
                    .monitoring_events_region_partition_change_enabled,
                site_region_change_enabled: self.monitoring_events_site_region_change_enabled,
            },
            notifications: SettingsNotificationsView {
                telegram: TelegramSettingsView {
                    enabled: self.telegram_enabled,
                    configured: self
                        .telegram_bot_token
                        .as_ref()
                        .is_some_and(|v| !v.trim().is_empty())
                        && !self.telegram_targets.is_empty(),
                    targets: self.telegram_targets.clone(),
                },
                web_push: WebPushSettingsView {
                    enabled: self.web_push_enabled,
                    vapid_public_key,
                },
            },
        }
    }
}

pub async fn init_db(db: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query(
        r#"
CREATE TABLE IF NOT EXISTS users (
  id TEXT PRIMARY KEY,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS catalog_configs (
  id TEXT PRIMARY KEY,
  country_id TEXT NOT NULL,
  region_id TEXT NULL,
  name TEXT NOT NULL,
  specs_json TEXT NOT NULL,
  price_amount REAL NOT NULL,
  price_currency TEXT NOT NULL,
  price_period TEXT NOT NULL,
  inventory_status TEXT NOT NULL,
  inventory_quantity INTEGER NOT NULL,
  checked_at TEXT NOT NULL,
  config_digest TEXT NOT NULL,
  lifecycle_state TEXT NOT NULL DEFAULT 'active',
  lifecycle_listed_at TEXT NOT NULL DEFAULT '1970-01-01T00:00:00Z',
  lifecycle_delisted_at TEXT NULL,
  lifecycle_last_seen_at TEXT NOT NULL DEFAULT '1970-01-01T00:00:00Z',
  lifecycle_listed_event_at TEXT NULL,
  source_pid TEXT NULL,
  source_fid TEXT NULL,
  source_gid TEXT NULL
);

CREATE TABLE IF NOT EXISTS catalog_url_cache (
  url_key TEXT PRIMARY KEY,
  url TEXT NOT NULL,
  config_ids_json TEXT NOT NULL,
  last_success_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS catalog_countries (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  sort_index INTEGER NOT NULL,
  has_regions INTEGER NOT NULL DEFAULT 0,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS catalog_regions (
  id TEXT PRIMARY KEY,
  country_id TEXT NOT NULL,
  name TEXT NOT NULL,
  location_name TEXT NULL,
  sort_index INTEGER NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS catalog_region_notices (
  url_key TEXT PRIMARY KEY,
  country_id TEXT NOT NULL,
  region_id TEXT NULL,
  text TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS catalog_topology_state (
  state_key TEXT PRIMARY KEY,
  source_url TEXT NOT NULL,
  last_topology_refresh_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS inventory_samples_1m (
  config_id TEXT NOT NULL,
  ts_minute TEXT NOT NULL,
  inventory_quantity INTEGER NOT NULL,
  PRIMARY KEY (config_id, ts_minute)
);

CREATE TABLE IF NOT EXISTS monitoring_configs (
  user_id TEXT NOT NULL,
  config_id TEXT NOT NULL,
  enabled INTEGER NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (user_id, config_id)
);

CREATE TABLE IF NOT EXISTS monitoring_partitions (
  user_id TEXT NOT NULL,
  partition_key TEXT NOT NULL,
  country_id TEXT NOT NULL,
  region_id TEXT NULL,
  enabled INTEGER NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (user_id, partition_key)
);

CREATE TABLE IF NOT EXISTS user_config_archives (
  user_id TEXT NOT NULL,
  config_id TEXT NOT NULL,
  cleaned_at TEXT NOT NULL,
  PRIMARY KEY (user_id, config_id)
);

CREATE TABLE IF NOT EXISTS settings (
  user_id TEXT PRIMARY KEY,
  poll_interval_minutes INTEGER NOT NULL,
  poll_jitter_pct REAL NOT NULL,
  site_base_url TEXT NULL,
  catalog_refresh_auto_interval_hours INTEGER NULL,
  monitoring_events_listed_enabled INTEGER NOT NULL DEFAULT 0,
  monitoring_events_partition_listed_enabled INTEGER NOT NULL DEFAULT 0,
  monitoring_events_site_listed_enabled INTEGER NOT NULL DEFAULT 0,
  monitoring_events_delisted_enabled INTEGER NOT NULL DEFAULT 0,
  monitoring_events_partition_catalog_change_enabled INTEGER NOT NULL DEFAULT 0,
  monitoring_events_region_partition_change_enabled INTEGER NOT NULL DEFAULT 0,
  monitoring_events_site_region_change_enabled INTEGER NOT NULL DEFAULT 0,
  telegram_enabled INTEGER NOT NULL,
  telegram_bot_token TEXT NULL,
  telegram_target TEXT NULL,
  telegram_targets_json TEXT NULL,
  web_push_enabled INTEGER NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS web_push_subscriptions (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL,
  endpoint TEXT NOT NULL,
  p256dh TEXT NOT NULL,
  auth TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS event_logs (
  id TEXT PRIMARY KEY,
  user_id TEXT NULL,
  ts TEXT NOT NULL,
  level TEXT NOT NULL,
  scope TEXT NOT NULL,
  message TEXT NOT NULL,
  meta_json TEXT NULL
);

CREATE TABLE IF NOT EXISTS notification_records (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL,
  created_at TEXT NOT NULL,
  kind TEXT NOT NULL,
  title TEXT NOT NULL,
  summary TEXT NOT NULL,
  partition_label TEXT NULL,
  telegram_status TEXT NOT NULL DEFAULT 'not_sent',
  web_push_status TEXT NOT NULL DEFAULT 'not_sent'
);

CREATE TABLE IF NOT EXISTS notification_record_items (
  id TEXT PRIMARY KEY,
  record_id TEXT NOT NULL,
  position INTEGER NOT NULL,
  config_id TEXT NULL,
  name TEXT NOT NULL,
  country_name TEXT NOT NULL,
  region_name TEXT NULL,
  specs_json TEXT NOT NULL,
  price_amount REAL NOT NULL,
  price_currency TEXT NOT NULL,
  price_period TEXT NOT NULL,
  inventory_status TEXT NOT NULL,
  inventory_quantity INTEGER NOT NULL,
  checked_at TEXT NOT NULL,
  lifecycle_state TEXT NOT NULL,
  lifecycle_listed_at TEXT NOT NULL,
  lifecycle_delisted_at TEXT NULL
);

CREATE TABLE IF NOT EXISTS notification_record_deliveries (
  id TEXT PRIMARY KEY,
  record_id TEXT NOT NULL,
  channel TEXT NOT NULL,
  target TEXT NOT NULL,
  status TEXT NOT NULL,
  error_message TEXT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS ops_events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  ts TEXT NOT NULL,
  event TEXT NOT NULL,
  data_json TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS ops_task_runs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  fid TEXT NOT NULL,
  gid TEXT NULL,
  started_at TEXT NOT NULL,
  ended_at TEXT NULL,
  ok INTEGER NOT NULL,
  fetch_action TEXT NOT NULL DEFAULT 'fetch',
  freshness_window_seconds INTEGER NULL,
  fetch_http_status INTEGER NULL,
  fetch_bytes INTEGER NULL,
  fetch_elapsed_ms INTEGER NULL,
  parse_produced_configs INTEGER NULL,
  parse_elapsed_ms INTEGER NULL,
  error_code TEXT NULL,
  error_message TEXT NULL
);

CREATE TABLE IF NOT EXISTS ops_notify_runs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  task_run_id INTEGER NOT NULL,
  ts TEXT NOT NULL,
  channel TEXT NOT NULL,
  result TEXT NOT NULL,
  error_message TEXT NULL
);

CREATE INDEX IF NOT EXISTS idx_event_logs_user_ts ON event_logs (user_id, ts DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_event_logs_ts ON event_logs (ts DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_notification_records_user_created ON notification_records (user_id, created_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_notification_record_items_record_position ON notification_record_items (record_id, position ASC);
CREATE INDEX IF NOT EXISTS idx_notification_record_deliveries_record_channel ON notification_record_deliveries (record_id, channel, created_at ASC, id ASC);
CREATE INDEX IF NOT EXISTS idx_notification_record_deliveries_channel_ts ON notification_record_deliveries (channel, created_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_inventory_samples_1m_ts ON inventory_samples_1m (ts_minute);
CREATE INDEX IF NOT EXISTS idx_catalog_url_cache_last_success_at ON catalog_url_cache (last_success_at DESC, url_key);
CREATE INDEX IF NOT EXISTS idx_catalog_countries_sort ON catalog_countries (sort_index, id);
CREATE INDEX IF NOT EXISTS idx_catalog_regions_country_sort ON catalog_regions (country_id, sort_index, id);
CREATE INDEX IF NOT EXISTS idx_monitoring_partitions_user_enabled_updated_at ON monitoring_partitions (user_id, enabled, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_monitoring_partitions_country_region_enabled ON monitoring_partitions (country_id, region_id, enabled);
CREATE INDEX IF NOT EXISTS idx_user_config_archives_user_cleaned_at ON user_config_archives (user_id, cleaned_at DESC);
CREATE INDEX IF NOT EXISTS idx_user_config_archives_config_id ON user_config_archives (config_id);

CREATE INDEX IF NOT EXISTS idx_ops_events_ts ON ops_events (ts DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_ops_task_runs_ended_at ON ops_task_runs (ended_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_ops_task_runs_key ON ops_task_runs (fid, gid, ended_at DESC);
CREATE INDEX IF NOT EXISTS idx_ops_notify_runs_task_run_id ON ops_notify_runs (task_run_id);
CREATE INDEX IF NOT EXISTS idx_ops_notify_runs_channel_ts ON ops_notify_runs (channel, ts DESC);
"#,
    )
    .execute(db)
    .await?;

    let site_listed_column_exists =
        column_exists(db, "settings", "monitoring_events_site_listed_enabled").await?;
    let partition_catalog_change_column_exists = column_exists(
        db,
        "settings",
        "monitoring_events_partition_catalog_change_enabled",
    )
    .await?;
    let region_partition_change_column_exists = column_exists(
        db,
        "settings",
        "monitoring_events_region_partition_change_enabled",
    )
    .await?;
    let site_region_change_column_exists = column_exists(
        db,
        "settings",
        "monitoring_events_site_region_change_enabled",
    )
    .await?;

    // Best-effort schema updates for older DBs.
    add_column_if_missing(
        db,
        "catalog_configs",
        "lifecycle_state",
        "TEXT NOT NULL DEFAULT 'active'",
    )
    .await?;
    add_column_if_missing(
        db,
        "catalog_configs",
        "lifecycle_listed_at",
        "TEXT NOT NULL DEFAULT '1970-01-01T00:00:00Z'",
    )
    .await?;
    add_column_if_missing(db, "catalog_configs", "lifecycle_delisted_at", "TEXT NULL").await?;
    add_column_if_missing(
        db,
        "catalog_configs",
        "lifecycle_last_seen_at",
        "TEXT NOT NULL DEFAULT '1970-01-01T00:00:00Z'",
    )
    .await?;
    let lifecycle_listed_event_at_added = add_column_if_missing(
        db,
        "catalog_configs",
        "lifecycle_listed_event_at",
        "TEXT NULL",
    )
    .await?;
    add_column_if_missing(db, "catalog_configs", "source_pid", "TEXT NULL").await?;
    add_column_if_missing(db, "catalog_configs", "source_fid", "TEXT NULL").await?;
    add_column_if_missing(db, "catalog_configs", "source_gid", "TEXT NULL").await?;

    add_column_if_missing(db, "ops_task_runs", "reason_counts_json", "TEXT NULL").await?;
    add_column_if_missing(
        db,
        "ops_task_runs",
        "cache_hit",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;

    add_column_if_missing(
        db,
        "settings",
        "catalog_refresh_auto_interval_hours",
        "INTEGER NULL",
    )
    .await?;
    add_column_if_missing(
        db,
        "settings",
        "monitoring_events_listed_enabled",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    add_column_if_missing(
        db,
        "settings",
        "monitoring_events_partition_listed_enabled",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    add_column_if_missing(
        db,
        "settings",
        "monitoring_events_site_listed_enabled",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    add_column_if_missing(
        db,
        "settings",
        "monitoring_events_delisted_enabled",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    add_column_if_missing(
        db,
        "settings",
        "monitoring_events_partition_catalog_change_enabled",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    add_column_if_missing(
        db,
        "settings",
        "monitoring_events_region_partition_change_enabled",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    add_column_if_missing(
        db,
        "settings",
        "monitoring_events_site_region_change_enabled",
        "INTEGER NOT NULL DEFAULT 0",
    )
    .await?;
    add_column_if_missing(db, "settings", "telegram_targets_json", "TEXT NULL").await?;
    if !site_listed_column_exists {
        sqlx::query(
            r#"
UPDATE settings
SET monitoring_events_site_listed_enabled = monitoring_events_listed_enabled
WHERE monitoring_events_listed_enabled != 0
"#,
        )
        .execute(db)
        .await?;
    }
    if !partition_catalog_change_column_exists {
        sqlx::query(
            r#"
UPDATE settings
SET monitoring_events_partition_catalog_change_enabled = monitoring_events_partition_listed_enabled
WHERE monitoring_events_partition_listed_enabled != 0
"#,
        )
        .execute(db)
        .await?;
    }
    if !region_partition_change_column_exists {
        sqlx::query(
            r#"
UPDATE settings
SET monitoring_events_region_partition_change_enabled = 0
"#,
        )
        .execute(db)
        .await?;
    }
    if !site_region_change_column_exists {
        sqlx::query(
            r#"
UPDATE settings
SET monitoring_events_site_region_change_enabled = monitoring_events_site_listed_enabled
WHERE monitoring_events_site_listed_enabled != 0
"#,
        )
        .execute(db)
        .await?;
    }
    add_column_if_missing(
        db,
        "ops_task_runs",
        "fetch_action",
        "TEXT NOT NULL DEFAULT 'fetch'",
    )
    .await?;
    add_column_if_missing(
        db,
        "ops_task_runs",
        "freshness_window_seconds",
        "INTEGER NULL",
    )
    .await?;

    // Backfill lifecycle timestamps for existing rows (idempotent).
    sqlx::query(
        r#"
UPDATE catalog_configs
SET
  lifecycle_state = COALESCE(NULLIF(lifecycle_state, ''), 'active'),
  lifecycle_listed_at = CASE
    WHEN lifecycle_listed_at IS NULL OR lifecycle_listed_at = '1970-01-01T00:00:00Z' THEN checked_at
    ELSE lifecycle_listed_at
  END,
  lifecycle_last_seen_at = CASE
    WHEN lifecycle_last_seen_at IS NULL OR lifecycle_last_seen_at = '1970-01-01T00:00:00Z' THEN checked_at
    ELSE lifecycle_last_seen_at
  END
"#,
    )
    .execute(db)
    .await?;

    if lifecycle_listed_event_at_added {
        sqlx::query(
            r#"
UPDATE catalog_configs
SET lifecycle_listed_event_at = CASE
  WHEN lifecycle_listed_at IS NULL OR lifecycle_listed_at = '1970-01-01T00:00:00Z' THEN checked_at
  ELSE lifecycle_listed_at
END
WHERE COALESCE(NULLIF(lifecycle_state, ''), 'active') = 'active'
  AND (lifecycle_listed_event_at IS NULL OR TRIM(lifecycle_listed_event_at) = '')
"#,
        )
        .execute(db)
        .await?;
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct CatalogTopologyStateRow {
    pub source_url: String,
    pub last_topology_refresh_at: String,
}

pub async fn replace_catalog_topology(
    db: &SqlitePool,
    source_url: &str,
    countries: &[Country],
    regions: &[Region],
) -> anyhow::Result<()> {
    let now = now_rfc3339();
    let mut tx = db.begin().await?;

    sqlx::query("DELETE FROM catalog_countries")
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM catalog_regions")
        .execute(&mut *tx)
        .await?;

    let region_country_ids = regions
        .iter()
        .map(|r| r.country_id.as_str())
        .collect::<std::collections::HashSet<_>>();

    for (idx, country) in countries.iter().enumerate() {
        sqlx::query(
            r#"
INSERT INTO catalog_countries (id, name, sort_index, has_regions, updated_at)
VALUES (?, ?, ?, ?, ?)
"#,
        )
        .bind(&country.id)
        .bind(&country.name)
        .bind(idx as i64)
        .bind(if region_country_ids.contains(country.id.as_str()) {
            1
        } else {
            0
        })
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }

    for (idx, region) in regions.iter().enumerate() {
        sqlx::query(
            r#"
INSERT INTO catalog_regions (id, country_id, name, location_name, sort_index, updated_at)
VALUES (?, ?, ?, ?, ?, ?)
"#,
        )
        .bind(&region.id)
        .bind(&region.country_id)
        .bind(&region.name)
        .bind(region.location_name.as_deref())
        .bind(idx as i64)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }

    let active_url_keys = countries
        .iter()
        .map(|country| format!("{}:0", country.id))
        .chain(
            regions
                .iter()
                .map(|region| format!("{}:{}", region.country_id, region.id)),
        )
        .collect::<Vec<_>>();

    sqlx::query(
        r#"
INSERT INTO catalog_topology_state (state_key, source_url, last_topology_refresh_at, updated_at)
VALUES ('default', ?, ?, ?)
ON CONFLICT(state_key) DO UPDATE SET
  source_url = excluded.source_url,
  last_topology_refresh_at = excluded.last_topology_refresh_at,
  updated_at = excluded.updated_at
"#,
    )
    .bind(source_url)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    if active_url_keys.is_empty() {
        sqlx::query("DELETE FROM catalog_region_notices")
            .execute(&mut *tx)
            .await?;
    } else {
        let placeholders = std::iter::repeat_n("?", active_url_keys.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql =
            format!("DELETE FROM catalog_region_notices WHERE url_key NOT IN ({placeholders})");
        let mut q = sqlx::query(&sql);
        for key in &active_url_keys {
            q = q.bind(key);
        }
        q.execute(&mut *tx).await?;
    }

    tx.commit().await?;
    Ok(())
}

pub async fn set_catalog_region_notice(
    db: &SqlitePool,
    fid: &str,
    gid: Option<&str>,
    text: Option<&str>,
) -> anyhow::Result<()> {
    let now = now_rfc3339();
    let url_key = crate::upstream::catalog_region_key(fid, gid);
    let text = text.map(str::trim).filter(|v| !v.is_empty());
    if let Some(text) = text {
        sqlx::query(
            r#"
INSERT INTO catalog_region_notices (url_key, country_id, region_id, text, updated_at)
VALUES (?, ?, ?, ?, ?)
ON CONFLICT(url_key) DO UPDATE SET
  country_id = excluded.country_id,
  region_id = excluded.region_id,
  text = excluded.text,
  updated_at = excluded.updated_at
"#,
        )
        .bind(&url_key)
        .bind(fid)
        .bind(gid)
        .bind(text)
        .bind(&now)
        .execute(db)
        .await?;
    } else {
        sqlx::query("DELETE FROM catalog_region_notices WHERE url_key = ?")
            .bind(&url_key)
            .execute(db)
            .await?;
    }
    Ok(())
}

pub async fn get_catalog_topology_state(
    db: &SqlitePool,
) -> anyhow::Result<Option<CatalogTopologyStateRow>> {
    let row = sqlx::query(
        r#"
SELECT source_url, last_topology_refresh_at
FROM catalog_topology_state
WHERE state_key = 'default'
"#,
    )
    .fetch_optional(db)
    .await?;
    Ok(row.map(|row| CatalogTopologyStateRow {
        source_url: row.get::<String, _>(0),
        last_topology_refresh_at: row.get::<String, _>(1),
    }))
}

pub async fn has_catalog_topology(db: &SqlitePool) -> anyhow::Result<bool> {
    let row = sqlx::query("SELECT COUNT(*) FROM catalog_countries")
        .fetch_one(db)
        .await?;
    Ok(row.get::<i64, _>(0) > 0)
}

pub async fn list_catalog_task_keys(
    db: &SqlitePool,
) -> anyhow::Result<Vec<(String, Option<String>)>> {
    let countries = sqlx::query(
        r#"
SELECT id
FROM catalog_countries
ORDER BY sort_index ASC, id ASC
"#,
    )
    .fetch_all(db)
    .await?;

    let regions = sqlx::query(
        r#"
SELECT country_id, id
FROM catalog_regions
ORDER BY sort_index ASC, id ASC
"#,
    )
    .fetch_all(db)
    .await?;

    let mut out = countries
        .into_iter()
        .map(|row| (row.get::<String, _>(0), None))
        .collect::<Vec<_>>();
    out.extend(
        regions
            .into_iter()
            .map(|row| (row.get::<String, _>(0), Some(row.get::<String, _>(1)))),
    );
    Ok(out)
}

pub async fn load_catalog_snapshot(
    db: &SqlitePool,
    source_url: &str,
) -> anyhow::Result<crate::upstream::CatalogSnapshot> {
    let country_rows = sqlx::query(
        r#"
SELECT id, name
FROM catalog_countries
ORDER BY sort_index ASC, id ASC
"#,
    )
    .fetch_all(db)
    .await?;
    let countries = country_rows
        .into_iter()
        .map(|row| Country {
            id: row.get::<String, _>(0),
            name: row.get::<String, _>(1),
        })
        .collect::<Vec<_>>();

    let region_rows = sqlx::query(
        r#"
SELECT id, country_id, name, location_name
FROM catalog_regions
ORDER BY sort_index ASC, id ASC
"#,
    )
    .fetch_all(db)
    .await?;
    let regions = region_rows
        .into_iter()
        .map(|row| Region {
            id: row.get::<String, _>(0),
            country_id: row.get::<String, _>(1),
            name: row.get::<String, _>(2),
            location_name: row.get::<Option<String>, _>(3),
        })
        .collect::<Vec<_>>();

    let notice_rows = sqlx::query(
        r#"
SELECT country_id, region_id, text
FROM catalog_region_notices
ORDER BY url_key ASC
"#,
    )
    .fetch_all(db)
    .await?;
    let region_notices = notice_rows
        .into_iter()
        .map(|row| RegionNotice {
            country_id: row.get::<String, _>(0),
            region_id: row.get::<Option<String>, _>(1),
            text: row.get::<String, _>(2),
        })
        .collect::<Vec<_>>();

    let active_url_keys = countries
        .iter()
        .map(|country| crate::upstream::catalog_region_key(&country.id, None))
        .chain(regions.iter().map(|region| {
            crate::upstream::catalog_region_key(&region.country_id, Some(region.id.as_str()))
        }))
        .collect::<std::collections::HashSet<_>>();

    let cache_rows = sqlx::query("SELECT url_key FROM catalog_url_cache")
        .fetch_all(db)
        .await?;
    let region_notice_initialized_keys = cache_rows
        .into_iter()
        .map(|row| row.get::<String, _>(0))
        .filter(|key| active_url_keys.contains(key))
        .collect::<std::collections::HashSet<_>>();

    let fetched_at = sqlx::query(
        r#"
SELECT COALESCE(
  (SELECT MAX(checked_at) FROM catalog_configs),
  (SELECT last_topology_refresh_at FROM catalog_topology_state WHERE state_key = 'default'),
  ?
)
"#,
    )
    .bind(now_rfc3339())
    .fetch_one(db)
    .await?
    .get::<String, _>(0);

    let topology_state = get_catalog_topology_state(db).await?;
    let effective_source_url = topology_state
        .as_ref()
        .map(|row| row.source_url.clone())
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| source_url.to_string());
    let topology_refreshed_at = topology_state
        .as_ref()
        .map(|row| row.last_topology_refresh_at.clone());
    let topology_status = if topology_refreshed_at.is_some() {
        "success".to_string()
    } else {
        "idle".to_string()
    };

    Ok(crate::upstream::CatalogSnapshot {
        countries,
        regions,
        region_notices,
        region_notice_initialized_keys,
        configs: Vec::new(),
        fetched_at,
        source_url: effective_source_url,
        topology_refreshed_at,
        topology_request_count: 0,
        topology_status,
        topology_message: None,
    })
}

async fn column_exists(db: &SqlitePool, table: &str, column: &str) -> anyhow::Result<bool> {
    let pragma = format!("PRAGMA table_info({table})");
    let rows = sqlx::query(&pragma).fetch_all(db).await?;
    Ok(rows
        .into_iter()
        .any(|row| row.get::<String, _>(1).trim() == column))
}

async fn add_column_if_missing(
    db: &SqlitePool,
    table: &str,
    column: &str,
    column_def: &str,
) -> anyhow::Result<bool> {
    let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {column_def}");
    match sqlx::query(&sql).execute(db).await {
        Ok(_) => Ok(true),
        Err(err) => {
            // SQLite emits: "duplicate column name: <col>"
            let msg = err.to_string();
            if msg.to_lowercase().contains("duplicate column name") {
                Ok(false)
            } else {
                Err(err.into())
            }
        }
    }
}

fn now_rfc3339() -> String {
    format_rfc3339(OffsetDateTime::now_utc())
}

fn notification_record_now_rfc3339() -> String {
    format_notification_record_rfc3339(OffsetDateTime::now_utc())
}

fn format_rfc3339(ts: OffsetDateTime) -> String {
    ts.to_offset(UtcOffset::UTC)
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn format_notification_record_rfc3339(ts: OffsetDateTime) -> String {
    ts.to_offset(UtcOffset::UTC)
        .format(CANONICAL_RFC3339)
        .unwrap_or_else(|_| {
            // Should not happen; keep response stable.
            "1970-01-01T00:00:00.000000000Z".to_string()
        })
}

fn floor_to_minute_utc(dt: OffsetDateTime) -> OffsetDateTime {
    let dt = dt.to_offset(UtcOffset::UTC);
    let dt = dt.replace_second(0).unwrap_or(dt);
    dt.replace_nanosecond(0).unwrap_or(dt)
}

fn floor_rfc3339_to_minute_utc(ts: &str) -> Option<String> {
    let parsed = OffsetDateTime::parse(ts, &Rfc3339).ok()?;
    let floored = floor_to_minute_utc(parsed);
    floored.format(&Rfc3339).ok()
}

pub async fn ensure_user(
    db: &SqlitePool,
    cfg: &RuntimeConfig,
    user_id: &str,
) -> anyhow::Result<SettingsRow> {
    let now = now_rfc3339();
    sqlx::query("INSERT OR IGNORE INTO users (id, created_at) VALUES (?, ?)")
        .bind(user_id)
        .bind(&now)
        .execute(db)
        .await?;

    sqlx::query(
        r#"INSERT OR IGNORE INTO settings (
            user_id,
            poll_interval_minutes,
            poll_jitter_pct,
            site_base_url,
            catalog_refresh_auto_interval_hours,
            monitoring_events_listed_enabled,
            monitoring_events_partition_listed_enabled,
            monitoring_events_site_listed_enabled,
            monitoring_events_delisted_enabled,
            monitoring_events_partition_catalog_change_enabled,
            monitoring_events_region_partition_change_enabled,
            monitoring_events_site_region_change_enabled,
            telegram_enabled,
            telegram_bot_token,
            telegram_target,
            telegram_targets_json,
            web_push_enabled,
            created_at,
            updated_at
        ) VALUES (?, ?, ?, NULL, ?, 0, 0, 0, 0, 0, 0, 0, 0, NULL, NULL, NULL, 0, ?, ?)"#,
    )
    .bind(user_id)
    .bind(cfg.default_poll_interval_minutes)
    .bind(cfg.default_poll_jitter_pct)
    .bind(FIXED_CATALOG_TOPOLOGY_REFRESH_INTERVAL_HOURS)
    .bind(&now)
    .bind(&now)
    .execute(db)
    .await?;

    get_settings(db, user_id).await
}

pub async fn get_settings(db: &SqlitePool, user_id: &str) -> anyhow::Result<SettingsRow> {
    let row = sqlx::query(
        r#"SELECT
            poll_interval_minutes,
            poll_jitter_pct,
            site_base_url,
            catalog_refresh_auto_interval_hours,
            monitoring_events_partition_catalog_change_enabled,
            monitoring_events_region_partition_change_enabled,
            monitoring_events_site_region_change_enabled,
            telegram_enabled,
            telegram_bot_token,
            telegram_target,
            telegram_targets_json,
            web_push_enabled,
            created_at,
            updated_at
        FROM settings
        WHERE user_id = ?"#,
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;

    Ok(SettingsRow {
        poll_interval_minutes: row.get::<i64, _>(0),
        poll_jitter_pct: row.get::<f64, _>(1),
        site_base_url: row.get::<Option<String>, _>(2),
        catalog_refresh_auto_interval_hours: row.get::<Option<i64>, _>(3),
        monitoring_events_partition_catalog_change_enabled: row.get::<i64, _>(4) != 0,
        monitoring_events_region_partition_change_enabled: row.get::<i64, _>(5) != 0,
        monitoring_events_site_region_change_enabled: row.get::<i64, _>(6) != 0,
        telegram_enabled: row.get::<i64, _>(7) != 0,
        telegram_bot_token: row.get::<Option<String>, _>(8),
        telegram_target: row.get::<Option<String>, _>(9),
        telegram_targets: telegram_targets_from_storage(
            row.get::<Option<String>, _>(10).as_deref(),
            row.get::<Option<String>, _>(9).as_deref(),
        ),
        web_push_enabled: row.get::<i64, _>(11) != 0,
        created_at: row.get::<String, _>(12),
        updated_at: row.get::<String, _>(13),
    })
}

pub async fn update_settings(
    db: &SqlitePool,
    user_id: &str,
    req: SettingsUpdateRequest,
) -> anyhow::Result<SettingsRow> {
    let now = now_rfc3339();

    let existing = get_settings(db, user_id).await?;
    let existing_bot_token = existing.telegram_bot_token;
    let existing_targets = existing.telegram_targets;
    let existing_partition_catalog_change_enabled =
        existing.monitoring_events_partition_catalog_change_enabled;
    let existing_region_partition_change_enabled =
        existing.monitoring_events_region_partition_change_enabled;
    let existing_site_region_change_enabled = existing.monitoring_events_site_region_change_enabled;

    let telegram_bot_token = req
        .notifications
        .telegram
        .bot_token
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .or(existing_bot_token);
    let telegram_targets = match req.notifications.telegram.targets {
        Some(targets) => normalize_telegram_targets(targets),
        None => existing_targets,
    };
    let telegram_target = telegram_targets.first().cloned();
    let telegram_targets_json = if telegram_targets.is_empty() {
        None
    } else {
        Some(serde_json::to_string(&telegram_targets)?)
    };

    let auto_interval_hours = existing.catalog_refresh_auto_interval_hours;
    let partition_catalog_change_enabled = req
        .monitoring_events
        .as_ref()
        .map(|v| v.partition_catalog_change_enabled)
        .unwrap_or(existing_partition_catalog_change_enabled);
    let region_partition_change_enabled = req
        .monitoring_events
        .as_ref()
        .map(|v| v.region_partition_change_enabled)
        .unwrap_or(existing_region_partition_change_enabled);
    let site_region_change_enabled = req
        .monitoring_events
        .as_ref()
        .map(|v| v.site_region_change_enabled)
        .unwrap_or(existing_site_region_change_enabled);

    sqlx::query(
        r#"UPDATE settings SET
            poll_interval_minutes = ?,
            poll_jitter_pct = ?,
            site_base_url = ?,
            catalog_refresh_auto_interval_hours = ?,
            monitoring_events_partition_catalog_change_enabled = ?,
            monitoring_events_region_partition_change_enabled = ?,
            monitoring_events_site_region_change_enabled = ?,
            telegram_enabled = ?,
            telegram_bot_token = ?,
            telegram_target = ?,
            telegram_targets_json = ?,
            web_push_enabled = ?,
            updated_at = ?
        WHERE user_id = ?"#,
    )
    .bind(req.poll.interval_minutes)
    .bind(req.poll.jitter_pct)
    .bind(
        req.site_base_url
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty()),
    )
    .bind(auto_interval_hours)
    .bind(if partition_catalog_change_enabled {
        1
    } else {
        0
    })
    .bind(if region_partition_change_enabled {
        1
    } else {
        0
    })
    .bind(if site_region_change_enabled { 1 } else { 0 })
    .bind(if req.notifications.telegram.enabled {
        1
    } else {
        0
    })
    .bind(telegram_bot_token)
    .bind(telegram_target)
    .bind(telegram_targets_json)
    .bind(if req.notifications.web_push.enabled {
        1
    } else {
        0
    })
    .bind(&now)
    .bind(user_id)
    .execute(db)
    .await?;

    get_settings(db, user_id).await
}

pub async fn list_enabled_monitoring_config_ids(
    db: &SqlitePool,
    user_id: &str,
) -> anyhow::Result<Vec<String>> {
    let rows =
        sqlx::query("SELECT config_id FROM monitoring_configs WHERE user_id = ? AND enabled = 1")
            .bind(user_id)
            .fetch_all(db)
            .await?;
    Ok(rows.into_iter().map(|r| r.get::<String, _>(0)).collect())
}

pub async fn set_monitoring_config_enabled(
    db: &SqlitePool,
    user_id: &str,
    config_id: &str,
    enabled: bool,
) -> anyhow::Result<()> {
    let now = now_rfc3339();
    sqlx::query(
        r#"
INSERT INTO monitoring_configs (user_id, config_id, enabled, created_at, updated_at)
VALUES (?, ?, ?, ?, ?)
ON CONFLICT(user_id, config_id) DO UPDATE SET
  enabled = excluded.enabled,
  updated_at = excluded.updated_at
"#,
    )
    .bind(user_id)
    .bind(config_id)
    .bind(if enabled { 1 } else { 0 })
    .bind(&now)
    .bind(&now)
    .execute(db)
    .await?;
    Ok(())
}

pub fn monitoring_partition_key(country_id: &str, region_id: Option<&str>) -> String {
    format!("{}::{}", country_id.trim(), region_id.unwrap_or("").trim())
}

pub fn normalize_monitoring_partition(
    country_id: &str,
    region_id: Option<&str>,
) -> Option<MonitoringPartitionView> {
    let country_id = country_id.trim();
    if country_id.is_empty() {
        return None;
    }

    let region_id = region_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(std::string::ToString::to_string);

    Some(MonitoringPartitionView {
        country_id: country_id.to_string(),
        region_id,
    })
}

pub async fn catalog_partition_exists(
    db: &SqlitePool,
    country_id: &str,
    region_id: Option<&str>,
) -> anyhow::Result<bool> {
    let country_id = country_id.trim();
    if country_id.is_empty() {
        return Ok(false);
    }

    let region_id = region_id.map(str::trim).filter(|value| !value.is_empty());
    let row = if let Some(region_id) = region_id {
        sqlx::query(
            r#"
SELECT 1
FROM catalog_regions
WHERE country_id = ? AND id = ?
UNION
SELECT 1
FROM catalog_configs
WHERE country_id = ? AND region_id = ? AND lifecycle_state = 'active'
LIMIT 1
"#,
        )
        .bind(country_id)
        .bind(region_id)
        .bind(country_id)
        .bind(region_id)
        .fetch_optional(db)
        .await?
    } else {
        sqlx::query(
            r#"
SELECT 1
FROM catalog_countries
WHERE id = ?
UNION
SELECT 1
FROM catalog_regions
WHERE country_id = ?
UNION
SELECT 1
FROM catalog_configs
WHERE country_id = ? AND region_id IS NULL AND lifecycle_state = 'active'
LIMIT 1
"#,
        )
        .bind(country_id)
        .bind(country_id)
        .bind(country_id)
        .fetch_optional(db)
        .await?
    };
    Ok(row.is_some())
}

pub async fn list_enabled_monitoring_partitions(
    db: &SqlitePool,
    user_id: &str,
) -> anyhow::Result<Vec<MonitoringPartitionView>> {
    let rows = sqlx::query(
        r#"
SELECT country_id, region_id
FROM monitoring_partitions
WHERE user_id = ? AND enabled = 1
ORDER BY country_id ASC, region_id ASC, partition_key ASC
"#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            normalize_monitoring_partition(
                &row.get::<String, _>(0),
                row.get::<Option<String>, _>(1).as_deref(),
            )
        })
        .collect())
}

pub async fn set_monitoring_partition_enabled(
    db: &SqlitePool,
    user_id: &str,
    country_id: &str,
    region_id: Option<&str>,
    enabled: bool,
) -> anyhow::Result<MonitoringPartitionView> {
    let partition = normalize_monitoring_partition(country_id, region_id)
        .ok_or_else(|| anyhow::anyhow!("invalid monitoring partition"))?;
    let now = now_rfc3339();
    let partition_key =
        monitoring_partition_key(&partition.country_id, partition.region_id.as_deref());
    sqlx::query(
        r#"
INSERT INTO monitoring_partitions (
  user_id,
  partition_key,
  country_id,
  region_id,
  enabled,
  created_at,
  updated_at
)
VALUES (?, ?, ?, ?, ?, ?, ?)
ON CONFLICT(user_id, partition_key) DO UPDATE SET
  enabled = excluded.enabled,
  country_id = excluded.country_id,
  region_id = excluded.region_id,
  updated_at = excluded.updated_at
"#,
    )
    .bind(user_id)
    .bind(partition_key)
    .bind(&partition.country_id)
    .bind(partition.region_id.as_deref())
    .bind(if enabled { 1 } else { 0 })
    .bind(&now)
    .bind(&now)
    .execute(db)
    .await?;

    Ok(MonitoringPartitionView {
        country_id: partition.country_id,
        region_id: partition.region_id,
    })
}

fn monitor_supported_for_country(country_id: &str) -> bool {
    country_id.trim() != "2"
}

fn parse_specs_json(specs_json: &str) -> Vec<Spec> {
    serde_json::from_str(specs_json).unwrap_or_default()
}

fn config_view_from_row(row: &sqlx::sqlite::SqliteRow) -> anyhow::Result<ConfigView> {
    let specs_json = row.get::<String, _>("specs_json");
    let specs = parse_specs_json(&specs_json);

    let country_id = row.get::<String, _>("country_id");
    let region_id = row.get::<Option<String>, _>("region_id");
    let lifecycle_state = row.get::<String, _>("lifecycle_state").trim().to_string();
    let listed_at = row.get::<String, _>("lifecycle_listed_at");
    let delisted_at = row.get::<Option<String>, _>("lifecycle_delisted_at");
    let cleanup_at = row.get::<Option<String>, _>("cleanup_at");

    Ok(ConfigView {
        id: row.get::<String, _>("id"),
        country_id: country_id.clone(),
        region_id,
        name: row.get::<String, _>("name"),
        specs,
        price: Money {
            amount: row.get::<f64, _>("price_amount"),
            currency: row.get::<String, _>("price_currency"),
            period: row.get::<String, _>("price_period"),
        },
        inventory: Inventory {
            status: row.get::<String, _>("inventory_status"),
            quantity: row.get::<i64, _>("inventory_quantity"),
            checked_at: row.get::<String, _>("checked_at"),
        },
        digest: row.get::<String, _>("config_digest"),
        lifecycle: ConfigLifecycleView {
            state: lifecycle_state,
            listed_at,
            delisted_at,
            cleanup_at,
        },
        monitor_supported: monitor_supported_for_country(&country_id),
        monitor_enabled: row.get::<i64, _>("monitor_enabled") != 0,
        source_pid: row.get::<Option<String>, _>("source_pid"),
        source_fid: row.get::<Option<String>, _>("source_fid"),
        source_gid: row.get::<Option<String>, _>("source_gid"),
    })
}

fn build_partition_label(country_name: &str, region_name: Option<&str>) -> Option<String> {
    let country_name = country_name.trim();
    if country_name.is_empty() {
        return None;
    }
    let region_name = region_name.map(str::trim).filter(|value| !value.is_empty());
    Some(match region_name {
        Some(region_name) => format!("{country_name} / {region_name}"),
        None => country_name.to_string(),
    })
}

fn notification_record_item_view_from_row(
    row: &sqlx::sqlite::SqliteRow,
) -> NotificationRecordItemView {
    let specs_json = row.get::<String, _>("specs_json");
    let country_name = row.get::<String, _>("country_name");
    let region_name = row.get::<Option<String>, _>("region_name");
    NotificationRecordItemView {
        config_id: row.get::<Option<String>, _>("config_id"),
        country_name: country_name.clone(),
        region_name: region_name.clone(),
        partition_label: build_partition_label(&country_name, region_name.as_deref()),
        name: row.get::<String, _>("name"),
        specs: parse_specs_json(&specs_json),
        price: Money {
            amount: row.get::<f64, _>("price_amount"),
            currency: row.get::<String, _>("price_currency"),
            period: row.get::<String, _>("price_period"),
        },
        inventory: Inventory {
            status: row.get::<String, _>("inventory_status"),
            quantity: row.get::<i64, _>("inventory_quantity"),
            checked_at: row.get::<String, _>("checked_at"),
        },
        lifecycle: ConfigLifecycleView {
            state: row.get::<String, _>("lifecycle_state"),
            listed_at: row.get::<String, _>("lifecycle_listed_at"),
            delisted_at: row.get::<Option<String>, _>("lifecycle_delisted_at"),
            cleanup_at: None,
        },
    }
}

pub async fn load_notification_record_item_snapshot(
    db: &SqlitePool,
    config_id: &str,
) -> anyhow::Result<Option<NotificationRecordItemView>> {
    let mut items = load_notification_record_item_snapshots(db, &[config_id.to_string()]).await?;
    Ok(items.pop())
}

pub async fn load_notification_record_item_snapshots(
    db: &SqlitePool,
    config_ids: &[String],
) -> anyhow::Result<Vec<NotificationRecordItemView>> {
    if config_ids.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders = std::iter::repeat_n("?", config_ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        r#"
SELECT
  c.id AS config_id,
  c.name,
  COALESCE(cc.name, c.country_id) AS country_name,
  COALESCE(cr.name, c.region_id) AS region_name,
  c.specs_json,
  c.price_amount,
  c.price_currency,
  c.price_period,
  c.inventory_status,
  c.inventory_quantity,
  c.checked_at,
  c.lifecycle_state,
  c.lifecycle_listed_at,
  c.lifecycle_delisted_at
FROM catalog_configs c
LEFT JOIN catalog_countries cc
  ON cc.id = c.country_id
LEFT JOIN catalog_regions cr
  ON cr.country_id = c.country_id AND cr.id = c.region_id
WHERE c.id IN ({placeholders})
"#
    );
    let mut query = sqlx::query(&sql);
    for config_id in config_ids {
        query = query.bind(config_id);
    }
    let rows = query.fetch_all(db).await?;
    let mut by_id = std::collections::HashMap::new();
    for row in rows {
        let config_id = row.get::<Option<String>, _>("config_id");
        if let Some(config_id) = config_id {
            by_id.insert(config_id, notification_record_item_view_from_row(&row));
        }
    }

    let mut items = Vec::with_capacity(config_ids.len());
    for config_id in config_ids {
        if let Some(item) = by_id.remove(config_id) {
            items.push(item);
        }
    }
    Ok(items)
}

pub async fn list_catalog_configs_view(
    db: &SqlitePool,
    user_id: &str,
    country_id: Option<&str>,
    region_id: Option<&str>,
) -> anyhow::Result<Vec<ConfigView>> {
    let mut sql = r#"
SELECT
  c.id,
  c.country_id,
  c.region_id,
  c.name,
  c.specs_json,
  c.price_amount,
  c.price_currency,
  c.price_period,
  c.inventory_status,
  c.inventory_quantity,
  c.checked_at,
  c.config_digest,
  c.lifecycle_state,
  c.lifecycle_listed_at,
  c.lifecycle_delisted_at,
  a.cleaned_at AS cleanup_at,
  c.source_pid,
  c.source_fid,
  c.source_gid,
  COALESCE(m.enabled, 0) AS monitor_enabled
FROM catalog_configs c
LEFT JOIN monitoring_configs m
  ON m.user_id = ? AND m.config_id = c.id
LEFT JOIN user_config_archives a
  ON a.user_id = ? AND a.config_id = c.id
WHERE 1 = 1
"#
    .to_string();

    if country_id.is_some() {
        sql.push_str(" AND c.country_id = ?\n");
    }
    if region_id.is_some() {
        sql.push_str(" AND c.region_id = ?\n");
    }

    sql.push_str(" ORDER BY c.country_id ASC, c.region_id ASC, c.price_amount ASC, c.id ASC");

    let mut q = sqlx::query(&sql).bind(user_id).bind(user_id);
    if let Some(v) = country_id {
        q = q.bind(v);
    }
    if let Some(v) = region_id {
        q = q.bind(v);
    }

    let rows = q.fetch_all(db).await?;
    rows.iter().map(config_view_from_row).collect()
}

pub async fn list_monitoring_configs_view(
    db: &SqlitePool,
    user_id: &str,
) -> anyhow::Result<Vec<ConfigView>> {
    let rows = sqlx::query(
        r#"
SELECT
  c.id,
  c.country_id,
  c.region_id,
  c.name,
  c.specs_json,
  c.price_amount,
  c.price_currency,
  c.price_period,
  c.inventory_status,
  c.inventory_quantity,
  c.checked_at,
  c.config_digest,
  c.lifecycle_state,
  c.lifecycle_listed_at,
  c.lifecycle_delisted_at,
  a.cleaned_at AS cleanup_at,
  c.source_pid,
  c.source_fid,
  c.source_gid,
  COALESCE(m.enabled, 0) AS monitor_enabled
FROM catalog_configs c
JOIN monitoring_configs m
  ON m.user_id = ? AND m.config_id = c.id AND m.enabled = 1
LEFT JOIN user_config_archives a
  ON a.user_id = ? AND a.config_id = c.id
ORDER BY c.country_id ASC, c.region_id ASC, c.price_amount ASC, c.id ASC
"#,
    )
    .bind(user_id)
    .bind(user_id)
    .fetch_all(db)
    .await?;
    rows.iter().map(config_view_from_row).collect()
}

pub async fn list_recent_listed_24h_view(
    db: &SqlitePool,
    user_id: &str,
) -> anyhow::Result<Vec<ConfigView>> {
    let cutoff = OffsetDateTime::now_utc()
        .saturating_sub(time::Duration::hours(24))
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());
    let rows = sqlx::query(
        r#"
SELECT
  c.id,
  c.country_id,
  c.region_id,
  c.name,
  c.specs_json,
  c.price_amount,
  c.price_currency,
  c.price_period,
  c.inventory_status,
  c.inventory_quantity,
  c.checked_at,
  c.config_digest,
  c.lifecycle_state,
  c.lifecycle_listed_at,
  c.lifecycle_delisted_at,
  a.cleaned_at AS cleanup_at,
  c.source_pid,
  c.source_fid,
  c.source_gid,
  COALESCE(m.enabled, 0) AS monitor_enabled
FROM catalog_configs c
LEFT JOIN monitoring_configs m
  ON m.user_id = ? AND m.config_id = c.id
LEFT JOIN user_config_archives a
  ON a.user_id = ? AND a.config_id = c.id
WHERE c.lifecycle_listed_at >= ?
ORDER BY c.lifecycle_listed_at DESC, c.id DESC
LIMIT 200
"#,
    )
    .bind(user_id)
    .bind(user_id)
    .bind(cutoff)
    .fetch_all(db)
    .await?;
    rows.iter().map(config_view_from_row).collect()
}

pub async fn get_catalog_latest_checked_at(db: &SqlitePool) -> anyhow::Result<Option<String>> {
    let row = sqlx::query("SELECT MAX(checked_at) FROM catalog_configs")
        .fetch_one(db)
        .await?;
    Ok(row.get::<Option<String>, _>(0))
}

pub async fn list_known_catalog_targets(
    db: &SqlitePool,
) -> anyhow::Result<Vec<(String, Option<String>)>> {
    let rows = sqlx::query(
        r#"
SELECT DISTINCT source_fid, source_gid
FROM catalog_configs
WHERE source_fid IS NOT NULL
  AND TRIM(source_fid) != ''
  AND lifecycle_state = 'active'
ORDER BY source_fid ASC, source_gid ASC
"#,
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let fid = row.get::<String, _>(0).trim().to_string();
            let gid = row
                .get::<Option<String>, _>(1)
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            (fid, gid)
        })
        .filter(|(fid, _)| !fid.is_empty())
        .collect())
}

pub async fn retire_catalog_targets(
    db: &SqlitePool,
    targets: &[(String, Option<String>)],
) -> anyhow::Result<Vec<String>> {
    if targets.is_empty() {
        return Ok(Vec::new());
    }

    let retired_at = now_rfc3339();
    let mut tx = db.begin().await?;
    let mut retired_ids = Vec::new();

    for (fid, gid) in targets {
        let gid = gid.as_deref();
        let rows = sqlx::query(
            r#"
SELECT id
FROM catalog_configs
WHERE source_fid = ?
  AND (
    (? IS NULL AND source_gid IS NULL)
    OR (? IS NOT NULL AND source_gid = ?)
  )
  AND lifecycle_state != 'delisted'
"#,
        )
        .bind(fid)
        .bind(gid)
        .bind(gid)
        .bind(gid)
        .fetch_all(&mut *tx)
        .await?;
        retired_ids.extend(rows.into_iter().map(|row| row.get::<String, _>(0)));

        sqlx::query(
            r#"
UPDATE catalog_configs
SET lifecycle_state = 'delisted',
    lifecycle_delisted_at = ?
WHERE source_fid = ?
  AND (
    (? IS NULL AND source_gid IS NULL)
    OR (? IS NOT NULL AND source_gid = ?)
  )
  AND lifecycle_state != 'delisted'
"#,
        )
        .bind(&retired_at)
        .bind(fid)
        .bind(gid)
        .bind(gid)
        .bind(gid)
        .execute(&mut *tx)
        .await?;

        sqlx::query("DELETE FROM catalog_url_cache WHERE url_key = ?")
            .bind(crate::upstream::catalog_region_key(fid, gid))
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok(retired_ids)
}

pub async fn get_global_catalog_refresh_interval_hours(
    db: &SqlitePool,
) -> anyhow::Result<Option<i64>> {
    let row = sqlx::query(
        r#"
SELECT MIN(catalog_refresh_auto_interval_hours)
FROM settings
WHERE catalog_refresh_auto_interval_hours IS NOT NULL
  AND catalog_refresh_auto_interval_hours > 0
"#,
    )
    .fetch_one(db)
    .await?;
    Ok(row.get::<Option<i64>, _>(0))
}

#[derive(Debug, Clone)]
pub struct CatalogUrlCacheRow {
    pub url_key: String,
    pub url: String,
    pub config_ids_json: String,
    pub last_success_at: String,
}

pub async fn get_catalog_url_cache(
    db: &SqlitePool,
    url_key: &str,
) -> anyhow::Result<Option<CatalogUrlCacheRow>> {
    let row = sqlx::query(
        r#"
SELECT url_key, url, config_ids_json, last_success_at
FROM catalog_url_cache
WHERE url_key = ?
"#,
    )
    .bind(url_key)
    .fetch_optional(db)
    .await?;
    Ok(row.map(|row| CatalogUrlCacheRow {
        url_key: row.get::<String, _>(0),
        url: row.get::<String, _>(1),
        config_ids_json: row.get::<String, _>(2),
        last_success_at: row.get::<String, _>(3),
    }))
}

#[derive(Debug, Clone)]
pub struct ApplyCatalogUrlResult {
    pub listed_ids: Vec<String>,
    pub listed_event_ids: Vec<String>,
    pub listed_pending_zero_stock_ids: Vec<String>,
    pub delisted_ids: Vec<String>,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CatalogUrlFetchHints<'a> {
    pub region_notice: Option<&'a str>,
    pub empty_result_authoritative: bool,
}

#[derive(Debug, Clone)]
pub struct ArchiveDelistedResult {
    pub archived_count: i64,
    pub archived_at: Option<String>,
    pub archived_ids: Vec<String>,
}

pub async fn archive_all_delisted_configs(
    db: &SqlitePool,
    user_id: &str,
) -> anyhow::Result<ArchiveDelistedResult> {
    let archived_at = now_rfc3339();
    let mut tx = db.begin().await?;
    let archived_ids = sqlx::query(
        r#"
INSERT INTO user_config_archives (user_id, config_id, cleaned_at)
SELECT ?, c.id, ?
FROM catalog_configs c
LEFT JOIN user_config_archives a
  ON a.user_id = ? AND a.config_id = c.id
WHERE c.lifecycle_state = 'delisted'
  AND a.config_id IS NULL
ORDER BY c.id ASC
RETURNING config_id
"#,
    )
    .bind(user_id)
    .bind(&archived_at)
    .bind(user_id)
    .fetch_all(&mut *tx)
    .await?
    .into_iter()
    .map(|r| r.get::<String, _>(0))
    .collect::<Vec<_>>();
    tx.commit().await?;

    if archived_ids.is_empty() {
        return Ok(ArchiveDelistedResult {
            archived_count: 0,
            archived_at: None,
            archived_ids,
        });
    }

    Ok(ArchiveDelistedResult {
        archived_count: archived_ids.len() as i64,
        archived_at: Some(archived_at),
        archived_ids,
    })
}

pub async fn apply_catalog_url_fetch_success(
    db: &SqlitePool,
    fid: &str,
    gid: Option<&str>,
    url_key: &str,
    url: &str,
    mut configs: Vec<crate::upstream::ConfigBase>,
    hints: CatalogUrlFetchHints<'_>,
) -> anyhow::Result<ApplyCatalogUrlResult> {
    let fetched_at = now_rfc3339();
    for c in configs.iter_mut() {
        c.inventory.checked_at = fetched_at.clone();
    }

    let mut tx = db.begin().await?;

    let prev_ids: std::collections::HashSet<String> = if let Some(row) =
        sqlx::query("SELECT config_ids_json FROM catalog_url_cache WHERE url_key = ?")
            .bind(url_key)
            .fetch_optional(&mut *tx)
            .await?
    {
        serde_json::from_str::<Vec<String>>(&row.get::<String, _>(0))
            .unwrap_or_default()
            .into_iter()
            .collect()
    } else {
        let q = sqlx::query(
            r#"
SELECT id
FROM catalog_configs
WHERE source_fid = ?
  AND lifecycle_state = 'active'
  AND (
    (? IS NULL AND source_gid IS NULL)
    OR (? IS NOT NULL AND source_gid = ?)
  )
"#,
        )
        .bind(fid)
        .bind(gid)
        .bind(gid)
        .bind(gid);
        let rows = q.fetch_all(&mut *tx).await?;
        rows.into_iter().map(|r| r.get::<String, _>(0)).collect()
    };

    // An empty parse is usually ambiguous: it could mean the upstream cart truly has no items, or
    // it could be an upstream HTML change/error page that our parser didn't catch. Only apply an
    // empty result when the fetch path could prove the upstream page structure still matches a
    // valid "no direct packages" state.
    if configs.is_empty() && !prev_ids.is_empty() && !hints.empty_result_authoritative {
        anyhow::bail!(
            "refusing to apply empty catalog config list for {url_key} (would delist {} ids)",
            prev_ids.len()
        );
    }

    let fetched_ids: std::collections::HashSet<String> =
        configs.iter().map(|c| c.id.clone()).collect();

    let existing_by_id: std::collections::HashMap<String, (String, Option<String>)> =
        if fetched_ids.is_empty() {
            std::collections::HashMap::new()
        } else {
            let placeholders = std::iter::repeat_n("?", fetched_ids.len())
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                r#"
SELECT id, lifecycle_state, lifecycle_listed_event_at
FROM catalog_configs
WHERE id IN ({placeholders})
"#
            );
            let mut q = sqlx::query(&sql);
            for id in &fetched_ids {
                q = q.bind(id);
            }
            q.fetch_all(&mut *tx)
                .await?
                .into_iter()
                .map(|row| {
                    (
                        row.get::<String, _>(0),
                        (
                            row.get::<String, _>(1),
                            row.get::<Option<String>, _>(2)
                                .filter(|v| !v.trim().is_empty()),
                        ),
                    )
                })
                .collect()
        };

    let listed_ids = fetched_ids
        .difference(&prev_ids)
        .cloned()
        .collect::<Vec<_>>();
    let listed_id_set = listed_ids
        .iter()
        .cloned()
        .collect::<std::collections::HashSet<_>>();
    let delisted_ids = prev_ids
        .difference(&fetched_ids)
        .cloned()
        .collect::<Vec<_>>();

    let mut listed_event_ids = Vec::new();
    let mut listed_pending_zero_stock_ids = Vec::new();
    for c in &configs {
        let is_new_lifecycle = listed_id_set.contains(&c.id);
        let existing = existing_by_id.get(&c.id);
        let should_emit_listed_event = c.inventory.quantity > 0
            && (is_new_lifecycle
                || existing.is_some_and(|(state, listed_event_at)| {
                    state == "active" && listed_event_at.is_none()
                }));

        if should_emit_listed_event {
            listed_event_ids.push(c.id.clone());
        }

        if is_new_lifecycle && c.inventory.quantity == 0 {
            listed_pending_zero_stock_ids.push(c.id.clone());
        }
    }

    if !configs.is_empty() {
        // Upsert all fetched configs and mark as active.
        for c in configs.iter() {
            sqlx::query(
                r#"
INSERT INTO catalog_configs (
  id, country_id, region_id, name, specs_json,
  price_amount, price_currency, price_period,
  inventory_status, inventory_quantity, checked_at,
  config_digest,
  lifecycle_state, lifecycle_listed_at, lifecycle_delisted_at, lifecycle_last_seen_at,
  lifecycle_listed_event_at,
  source_pid, source_fid, source_gid
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'active', ?, NULL, ?, ?, ?, ?, ?)
ON CONFLICT(id) DO UPDATE SET
  country_id = excluded.country_id,
  region_id = excluded.region_id,
  name = excluded.name,
  specs_json = excluded.specs_json,
  price_amount = excluded.price_amount,
  price_currency = excluded.price_currency,
  price_period = excluded.price_period,
  inventory_status = excluded.inventory_status,
  inventory_quantity = excluded.inventory_quantity,
  checked_at = excluded.checked_at,
  config_digest = excluded.config_digest,
  lifecycle_state = 'active',
  lifecycle_delisted_at = NULL,
  lifecycle_last_seen_at = excluded.lifecycle_last_seen_at,
  lifecycle_listed_at = CASE
    WHEN catalog_configs.lifecycle_state = 'delisted' THEN excluded.lifecycle_listed_at
    WHEN catalog_configs.lifecycle_listed_at IS NULL OR catalog_configs.lifecycle_listed_at = '1970-01-01T00:00:00Z' THEN excluded.lifecycle_listed_at
    ELSE catalog_configs.lifecycle_listed_at
  END,
  lifecycle_listed_event_at = CASE
    WHEN catalog_configs.lifecycle_state = 'delisted' THEN excluded.lifecycle_listed_event_at
    WHEN catalog_configs.lifecycle_listed_event_at IS NULL AND excluded.inventory_quantity > 0 THEN excluded.lifecycle_listed_event_at
    ELSE catalog_configs.lifecycle_listed_event_at
  END,
  source_pid = COALESCE(excluded.source_pid, catalog_configs.source_pid),
  source_fid = excluded.source_fid,
  source_gid = excluded.source_gid
"#,
            )
            .bind(&c.id)
            .bind(&c.country_id)
            .bind(c.region_id.as_deref())
            .bind(&c.name)
            .bind(serde_json::to_string(&c.specs)?)
            .bind(c.price.amount)
            .bind(&c.price.currency)
            .bind(&c.price.period)
            .bind(&c.inventory.status)
            .bind(c.inventory.quantity)
            .bind(&c.inventory.checked_at)
            .bind(&c.digest)
            .bind(&fetched_at)
            .bind(&fetched_at)
            .bind(if c.inventory.quantity > 0 {
                Some(fetched_at.as_str())
            } else {
                None
            })
            .bind(c.source_pid.as_deref())
            .bind(c.source_fid.as_deref())
            .bind(c.source_gid.as_deref())
            .execute(&mut *tx)
            .await?;

            // Best-effort: write minute history samples without affecting current inventory availability.
            if let Some(ts_minute) = floor_rfc3339_to_minute_utc(&c.inventory.checked_at) {
                let _ = sqlx::query(
                    r#"
INSERT INTO inventory_samples_1m (config_id, ts_minute, inventory_quantity)
VALUES (?, ?, ?)
ON CONFLICT(config_id, ts_minute) DO UPDATE SET
  inventory_quantity = excluded.inventory_quantity
"#,
                )
                .bind(&c.id)
                .bind(ts_minute)
                .bind(c.inventory.quantity.max(0))
                .execute(&mut *tx)
                .await;
            }
        }
    }

    if !fetched_ids.is_empty() {
        let placeholders = std::iter::repeat_n("?", fetched_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!("DELETE FROM user_config_archives WHERE config_id IN ({placeholders})");
        let mut q = sqlx::query(&sql);
        for id in &fetched_ids {
            q = q.bind(id);
        }
        q.execute(&mut *tx).await?;
    }

    if !delisted_ids.is_empty() {
        // Mark configs as delisted (one success-miss = delist).
        let placeholders = std::iter::repeat_n("?", delisted_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            r#"
UPDATE catalog_configs
SET lifecycle_state = 'delisted',
    lifecycle_delisted_at = ?,
    lifecycle_listed_event_at = NULL
WHERE id IN ({placeholders})
  AND lifecycle_state != 'delisted'
"#
        );
        let mut q = sqlx::query(&sql).bind(&fetched_at);
        for id in &delisted_ids {
            q = q.bind(id);
        }
        q.execute(&mut *tx).await?;
    }

    let ids_json =
        serde_json::to_string(&configs.iter().map(|c| c.id.clone()).collect::<Vec<_>>())?;
    sqlx::query(
        r#"
INSERT INTO catalog_url_cache (url_key, url, config_ids_json, last_success_at, updated_at)
VALUES (?, ?, ?, ?, ?)
ON CONFLICT(url_key) DO UPDATE SET
  url = excluded.url,
  config_ids_json = excluded.config_ids_json,
  last_success_at = excluded.last_success_at,
  updated_at = excluded.updated_at
"#,
    )
    .bind(url_key)
    .bind(url)
    .bind(ids_json)
    .bind(&fetched_at)
    .bind(&fetched_at)
    .execute(&mut *tx)
    .await?;

    let notice = hints
        .region_notice
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(notice) = notice {
        sqlx::query(
            r#"
INSERT INTO catalog_region_notices (url_key, country_id, region_id, text, updated_at)
VALUES (?, ?, ?, ?, ?)
ON CONFLICT(url_key) DO UPDATE SET
  country_id = excluded.country_id,
  region_id = excluded.region_id,
  text = excluded.text,
  updated_at = excluded.updated_at
"#,
        )
        .bind(url_key)
        .bind(fid)
        .bind(gid)
        .bind(notice)
        .bind(&fetched_at)
        .execute(&mut *tx)
        .await?;
    } else {
        sqlx::query("DELETE FROM catalog_region_notices WHERE url_key = ?")
            .bind(url_key)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;

    Ok(ApplyCatalogUrlResult {
        listed_ids,
        listed_event_ids,
        listed_pending_zero_stock_ids,
        delisted_ids,
        fetched_at,
    })
}

pub async fn upsert_catalog_configs(
    db: &SqlitePool,
    configs: &[crate::upstream::ConfigBase],
) -> anyhow::Result<()> {
    let mut tx = db.begin().await?;
    let mut active_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for c in configs {
        active_ids.insert(c.id.clone());
        sqlx::query(
            r#"
INSERT INTO catalog_configs (
  id, country_id, region_id, name, specs_json,
  price_amount, price_currency, price_period,
  inventory_status, inventory_quantity, checked_at,
  config_digest,
  lifecycle_state, lifecycle_listed_at, lifecycle_delisted_at, lifecycle_last_seen_at,
  lifecycle_listed_event_at,
  source_pid, source_fid, source_gid
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'active', ?, NULL, ?, ?, ?, ?, ?)
ON CONFLICT(id) DO UPDATE SET
  country_id = excluded.country_id,
  region_id = excluded.region_id,
  name = excluded.name,
  specs_json = excluded.specs_json,
  price_amount = excluded.price_amount,
  price_currency = excluded.price_currency,
  price_period = excluded.price_period,
  inventory_status = excluded.inventory_status,
  inventory_quantity = excluded.inventory_quantity,
  checked_at = excluded.checked_at,
  config_digest = excluded.config_digest,
  lifecycle_state = 'active',
  lifecycle_delisted_at = NULL,
  lifecycle_last_seen_at = excluded.checked_at,
  lifecycle_listed_at = CASE
    WHEN catalog_configs.lifecycle_state = 'delisted' THEN excluded.checked_at
    WHEN catalog_configs.lifecycle_listed_at IS NULL OR catalog_configs.lifecycle_listed_at = '1970-01-01T00:00:00Z' THEN excluded.checked_at
    ELSE catalog_configs.lifecycle_listed_at
  END,
  lifecycle_listed_event_at = CASE
    WHEN catalog_configs.lifecycle_state = 'delisted' THEN excluded.lifecycle_listed_event_at
    ELSE COALESCE(catalog_configs.lifecycle_listed_event_at, excluded.lifecycle_listed_event_at)
  END,
  source_pid = COALESCE(excluded.source_pid, catalog_configs.source_pid),
  source_fid = excluded.source_fid,
  source_gid = excluded.source_gid
"#,
        )
        .bind(&c.id)
        .bind(&c.country_id)
        .bind(c.region_id.as_deref())
        .bind(&c.name)
        .bind(serde_json::to_string(&c.specs)?)
        .bind(c.price.amount)
        .bind(&c.price.currency)
        .bind(&c.price.period)
        .bind(&c.inventory.status)
        .bind(c.inventory.quantity)
        .bind(&c.inventory.checked_at)
        .bind(&c.digest)
        .bind(&c.inventory.checked_at)
        .bind(&c.inventory.checked_at)
        .bind(&c.inventory.checked_at)
        .bind(c.source_pid.as_deref())
        .bind(c.source_fid.as_deref())
        .bind(c.source_gid.as_deref())
        .execute(&mut *tx)
        .await?;

        // Best-effort: write minute history samples without affecting current inventory availability.
        if let Some(ts_minute) = floor_rfc3339_to_minute_utc(&c.inventory.checked_at) {
            let _ = sqlx::query(
                r#"
INSERT INTO inventory_samples_1m (config_id, ts_minute, inventory_quantity)
VALUES (?, ?, ?)
ON CONFLICT(config_id, ts_minute) DO UPDATE SET
  inventory_quantity = excluded.inventory_quantity
"#,
            )
            .bind(&c.id)
            .bind(ts_minute)
            .bind(c.inventory.quantity.max(0))
            .execute(&mut *tx)
            .await;
        }
    }

    if !active_ids.is_empty() {
        let placeholders = std::iter::repeat_n("?", active_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!("DELETE FROM user_config_archives WHERE config_id IN ({placeholders})");
        let mut q = sqlx::query(&sql);
        for id in &active_ids {
            q = q.bind(id);
        }
        q.execute(&mut *tx).await?;
    }

    tx.commit().await?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct InventorySample1mRow {
    pub config_id: String,
    pub ts_minute: String,
    pub inventory_quantity: i64,
}

pub async fn list_inventory_samples_1m(
    db: &SqlitePool,
    config_ids: &[String],
    window_from: &str,
    window_to: &str,
) -> anyhow::Result<Vec<InventorySample1mRow>> {
    if config_ids.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders = std::iter::repeat_n("?", config_ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        r#"SELECT config_id, ts_minute, inventory_quantity
           FROM inventory_samples_1m
           WHERE config_id IN ({placeholders})
             AND ts_minute >= ?
             AND ts_minute <= ?
           ORDER BY config_id ASC, ts_minute ASC"#
    );

    let mut query = sqlx::query(&sql);
    for id in config_ids {
        query = query.bind(id);
    }
    query = query.bind(window_from).bind(window_to);

    let rows = query.fetch_all(db).await?;
    Ok(rows
        .into_iter()
        .map(|row| InventorySample1mRow {
            config_id: row.get::<String, _>(0),
            ts_minute: row.get::<String, _>(1),
            inventory_quantity: row.get::<i64, _>(2),
        })
        .collect())
}

pub async fn insert_log(
    db: &SqlitePool,
    user_id: Option<&str>,
    level: &str,
    scope: &str,
    message: &str,
    meta: Option<serde_json::Value>,
) -> anyhow::Result<()> {
    let id = Uuid::new_v4().to_string();
    let ts = now_rfc3339();
    sqlx::query(
        "INSERT INTO event_logs (id, user_id, ts, level, scope, message, meta_json) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(user_id)
    .bind(ts)
    .bind(level)
    .bind(scope)
    .bind(message)
    .bind(meta.map(|v| v.to_string()))
    .execute(db)
    .await?;
    Ok(())
}

pub async fn list_logs(
    db: &SqlitePool,
    user_id: &str,
    level: Option<&str>,
    cursor: Option<&str>,
    limit: i64,
) -> anyhow::Result<(Vec<LogEntryView>, Option<String>)> {
    let (cursor_ts, cursor_id) = cursor
        .and_then(|c| {
            // Cursor format: "<RFC3339 ts>:<id>".
            // RFC3339 timestamps contain `:` (e.g. "2026-01-19T00:00:00Z"), so we must split
            // from the right to preserve the timestamp portion.
            let mut parts = c.rsplitn(2, ':');
            let id = parts.next()?.to_string();
            let ts = parts.next()?.to_string();
            Some((ts, id))
        })
        .unwrap_or(("9999-12-31T23:59:59Z".to_string(), "zzzz".to_string()));

    let mut q = String::from(
        r#"SELECT id, ts, level, scope, message, meta_json
           FROM event_logs
           WHERE user_id = ?
             AND (ts < ? OR (ts = ? AND id < ?))"#,
    );
    if level.is_some() {
        q.push_str(" AND level = ?");
    }
    q.push_str(" ORDER BY ts DESC, id DESC LIMIT ?");

    let mut query = sqlx::query(&q)
        .bind(user_id)
        .bind(&cursor_ts)
        .bind(&cursor_ts)
        .bind(&cursor_id);
    if let Some(level) = level {
        query = query.bind(level);
    }
    query = query.bind(limit + 1);

    let rows = query.fetch_all(db).await?;
    let mut items = Vec::new();
    for row in rows.iter().take(limit as usize) {
        let meta_json = row.get::<Option<String>, _>(5);
        let meta = meta_json.and_then(|v| serde_json::from_str(&v).ok());
        items.push(LogEntryView {
            id: row.get::<String, _>(0),
            ts: row.get::<String, _>(1),
            level: row.get::<String, _>(2),
            scope: row.get::<String, _>(3),
            message: row.get::<String, _>(4),
            meta,
        });
    }

    let next_cursor = if rows.len() as i64 > limit {
        let last = items.last().unwrap();
        Some(format!("{}:{}", last.ts, last.id))
    } else {
        None
    };

    Ok((items, next_cursor))
}

fn notification_record_view_from_row(
    row: &sqlx::sqlite::SqliteRow,
    telegram_deliveries: Vec<NotificationRecordDeliveryView>,
    items: Vec<NotificationRecordItemView>,
) -> NotificationRecordView {
    NotificationRecordView {
        id: row.get::<String, _>("id"),
        created_at: row.get::<String, _>("created_at"),
        kind: row.get::<String, _>("kind"),
        title: row.get::<String, _>("title"),
        summary: row.get::<String, _>("summary"),
        partition_label: row.get::<Option<String>, _>("partition_label"),
        telegram_status: row.get::<String, _>("telegram_status"),
        web_push_status: row.get::<String, _>("web_push_status"),
        telegram_deliveries,
        items,
    }
}

async fn load_notification_record_items_by_record_ids(
    db: &SqlitePool,
    record_ids: &[String],
) -> anyhow::Result<std::collections::HashMap<String, Vec<NotificationRecordItemView>>> {
    if record_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    let placeholders = std::iter::repeat_n("?", record_ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        r#"
SELECT
  record_id,
  config_id,
  name,
  country_name,
  region_name,
  specs_json,
  price_amount,
  price_currency,
  price_period,
  inventory_status,
  inventory_quantity,
  checked_at,
  lifecycle_state,
  lifecycle_listed_at,
  lifecycle_delisted_at
FROM notification_record_items
WHERE record_id IN ({placeholders})
ORDER BY record_id ASC, position ASC, id ASC
"#
    );
    let mut query = sqlx::query(&sql);
    for record_id in record_ids {
        query = query.bind(record_id);
    }

    let rows = query.fetch_all(db).await?;
    let mut out = std::collections::HashMap::new();
    for row in rows {
        let record_id = row.get::<String, _>("record_id");
        out.entry(record_id)
            .or_insert_with(Vec::new)
            .push(notification_record_item_view_from_row(&row));
    }
    Ok(out)
}

pub async fn insert_notification_record(
    db: &SqlitePool,
    user_id: &str,
    draft: &NotificationRecordDraft,
) -> anyhow::Result<String> {
    let id = Uuid::new_v4().to_string();
    let created_at = notification_record_now_rfc3339();
    let mut tx = db.begin().await?;

    sqlx::query(
        r#"
INSERT INTO notification_records (
  id,
  user_id,
  created_at,
  kind,
  title,
  summary,
  partition_label,
  telegram_status,
  web_push_status
)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
"#,
    )
    .bind(&id)
    .bind(user_id)
    .bind(&created_at)
    .bind(&draft.kind)
    .bind(&draft.title)
    .bind(&draft.summary)
    .bind(draft.partition_label.as_deref())
    .bind(&draft.telegram_status)
    .bind(&draft.web_push_status)
    .execute(&mut *tx)
    .await?;

    for (position, item) in draft.items.iter().enumerate() {
        let item_id = Uuid::new_v4().to_string();
        let specs_json = serde_json::to_string(&item.specs)?;
        sqlx::query(
            r#"
INSERT INTO notification_record_items (
  id,
  record_id,
  position,
  config_id,
  name,
  country_name,
  region_name,
  specs_json,
  price_amount,
  price_currency,
  price_period,
  inventory_status,
  inventory_quantity,
  checked_at,
  lifecycle_state,
  lifecycle_listed_at,
  lifecycle_delisted_at
)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
"#,
        )
        .bind(&item_id)
        .bind(&id)
        .bind(position as i64)
        .bind(item.config_id.as_deref())
        .bind(&item.name)
        .bind(&item.country_name)
        .bind(item.region_name.as_deref())
        .bind(&specs_json)
        .bind(item.price.amount)
        .bind(&item.price.currency)
        .bind(&item.price.period)
        .bind(&item.inventory.status)
        .bind(item.inventory.quantity)
        .bind(&item.inventory.checked_at)
        .bind(&item.lifecycle.state)
        .bind(&item.lifecycle.listed_at)
        .bind(item.lifecycle.delisted_at.as_deref())
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(id)
}

pub async fn replace_notification_record_deliveries(
    db: &SqlitePool,
    record_id: &str,
    channel: &str,
    deliveries: &[NotificationRecordDeliveryView],
) -> anyhow::Result<()> {
    let now = now_rfc3339();
    let mut tx = db.begin().await?;
    sqlx::query("DELETE FROM notification_record_deliveries WHERE record_id = ? AND channel = ?")
        .bind(record_id)
        .bind(channel)
        .execute(&mut *tx)
        .await?;

    for delivery in deliveries {
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
INSERT INTO notification_record_deliveries (
  id,
  record_id,
  channel,
  target,
  status,
  error_message,
  created_at,
  updated_at
)
VALUES (?, ?, ?, ?, ?, ?, ?, ?)
"#,
        )
        .bind(&id)
        .bind(record_id)
        .bind(channel)
        .bind(&delivery.target)
        .bind(&delivery.status)
        .bind(delivery.error.as_deref())
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

pub async fn update_notification_record_channel_status(
    db: &SqlitePool,
    record_id: &str,
    channel: &str,
    status: &str,
) -> anyhow::Result<()> {
    let sql = match channel {
        "telegram" => "UPDATE notification_records SET telegram_status = ? WHERE id = ?",
        "webPush" => "UPDATE notification_records SET web_push_status = ? WHERE id = ?",
        other => anyhow::bail!("unknown notification channel: {other}"),
    };
    sqlx::query(sql)
        .bind(status)
        .bind(record_id)
        .execute(db)
        .await?;
    Ok(())
}

async fn load_notification_record_deliveries_by_record_ids(
    db: &SqlitePool,
    record_ids: &[String],
) -> anyhow::Result<std::collections::HashMap<String, Vec<NotificationRecordDeliveryView>>> {
    if record_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    let placeholders = std::iter::repeat_n("?", record_ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        r#"
SELECT record_id, channel, target, status, error_message
FROM notification_record_deliveries
WHERE record_id IN ({placeholders})
ORDER BY record_id ASC, created_at ASC, id ASC
"#
    );

    let mut query = sqlx::query(&sql);
    for record_id in record_ids {
        query = query.bind(record_id);
    }

    let rows = query.fetch_all(db).await?;
    let mut out = std::collections::HashMap::new();
    for row in rows {
        let record_id = row.get::<String, _>("record_id");
        out.entry(record_id)
            .or_insert_with(Vec::new)
            .push(NotificationRecordDeliveryView {
                channel: row.get::<String, _>("channel"),
                target: row.get::<String, _>("target"),
                status: row.get::<String, _>("status"),
                error: row.get::<Option<String>, _>("error_message"),
            });
    }
    Ok(out)
}

pub async fn list_notification_records(
    db: &SqlitePool,
    user_id: &str,
    cursor: Option<&str>,
    limit: i64,
) -> anyhow::Result<(Vec<NotificationRecordView>, Option<String>)> {
    let (cursor_ts, cursor_id) = cursor
        .and_then(|c| {
            let mut parts = c.rsplitn(2, ':');
            let id = parts.next()?.trim().to_string();
            let ts = parts.next()?.trim();
            let parsed = OffsetDateTime::parse(ts, &Rfc3339).ok()?;
            Some((format_notification_record_rfc3339(parsed), id))
        })
        .unwrap_or((
            NOTIFICATION_RECORD_CURSOR_MAX_TS.to_string(),
            "zzzz".to_string(),
        ));

    let rows = sqlx::query(
        r#"
SELECT id, created_at, kind, title, summary, partition_label, telegram_status, web_push_status
FROM notification_records
WHERE user_id = ?
  AND (created_at < ? OR (created_at = ? AND id < ?))
ORDER BY created_at DESC, id DESC
LIMIT ?
"#,
    )
    .bind(user_id)
    .bind(&cursor_ts)
    .bind(&cursor_ts)
    .bind(&cursor_id)
    .bind(limit + 1)
    .fetch_all(db)
    .await?;

    let visible_rows = rows.iter().take(limit as usize).collect::<Vec<_>>();
    let record_ids = visible_rows
        .iter()
        .map(|row| row.get::<String, _>("id"))
        .collect::<Vec<_>>();
    let mut items_by_record = load_notification_record_items_by_record_ids(db, &record_ids).await?;
    let mut deliveries_by_record =
        load_notification_record_deliveries_by_record_ids(db, &record_ids).await?;

    let mut items = Vec::with_capacity(visible_rows.len());
    for row in visible_rows {
        let record_id = row.get::<String, _>("id");
        items.push(notification_record_view_from_row(
            row,
            deliveries_by_record.remove(&record_id).unwrap_or_default(),
            items_by_record.remove(&record_id).unwrap_or_default(),
        ));
    }

    let next_cursor = if rows.len() as i64 > limit {
        let last = items
            .last()
            .expect("visible items exist when next cursor exists");
        Some(format!("{}:{}", last.created_at, last.id))
    } else {
        None
    };

    Ok((items, next_cursor))
}

pub async fn get_notification_record(
    db: &SqlitePool,
    user_id: &str,
    record_id: &str,
) -> anyhow::Result<Option<NotificationRecordView>> {
    let row = sqlx::query(
        r#"
SELECT id, created_at, kind, title, summary, partition_label, telegram_status, web_push_status
FROM notification_records
WHERE user_id = ? AND id = ?
"#,
    )
    .bind(user_id)
    .bind(record_id)
    .fetch_optional(db)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let mut items_by_record =
        load_notification_record_items_by_record_ids(db, &[record_id.to_string()]).await?;
    let mut deliveries_by_record =
        load_notification_record_deliveries_by_record_ids(db, &[record_id.to_string()]).await?;
    Ok(Some(notification_record_view_from_row(
        &row,
        deliveries_by_record.remove(record_id).unwrap_or_default(),
        items_by_record.remove(record_id).unwrap_or_default(),
    )))
}

pub async fn cleanup_notification_records(
    db: &SqlitePool,
    retention_days: i64,
    max_rows: i64,
) -> anyhow::Result<()> {
    let mut tx = db.begin().await?;

    if retention_days > 0 {
        let cutoff = OffsetDateTime::now_utc().saturating_sub(time::Duration::days(retention_days));
        sqlx::query("DELETE FROM notification_records WHERE created_at < ?")
            .bind(format_notification_record_rfc3339(cutoff))
            .execute(&mut *tx)
            .await?;
    }
    if max_rows > 0 {
        sqlx::query(
            r#"
DELETE FROM notification_records
WHERE id IN (
  SELECT id
  FROM (
    SELECT
      id,
      ROW_NUMBER() OVER (
        PARTITION BY user_id
        ORDER BY created_at DESC, id DESC
      ) AS row_num
    FROM notification_records
  ) ranked
  WHERE row_num > ?
)"#,
        )
        .bind(max_rows)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query(
        r#"
DELETE FROM notification_record_items
WHERE NOT EXISTS (
  SELECT 1 FROM notification_records
  WHERE notification_records.id = notification_record_items.record_id
)"#,
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
DELETE FROM notification_record_deliveries
WHERE NOT EXISTS (
  SELECT 1 FROM notification_records
  WHERE notification_records.id = notification_record_deliveries.record_id
)"#,
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

pub async fn insert_web_push_subscription(
    db: &SqlitePool,
    user_id: &str,
    req: WebPushSubscribeRequest,
) -> anyhow::Result<String> {
    let id = Uuid::new_v4().to_string();
    let now = now_rfc3339();
    sqlx::query(
        r#"INSERT INTO web_push_subscriptions (id, user_id, endpoint, p256dh, auth, created_at)
           VALUES (?, ?, ?, ?, ?, ?)"#,
    )
    .bind(&id)
    .bind(user_id)
    .bind(req.subscription.endpoint)
    .bind(req.subscription.keys.p256dh)
    .bind(req.subscription.keys.auth)
    .bind(now)
    .execute(db)
    .await?;
    Ok(id)
}

pub async fn get_latest_web_push_subscription(
    db: &SqlitePool,
    user_id: &str,
) -> anyhow::Result<Option<WebPushSubscription>> {
    let row = sqlx::query(
        r#"SELECT endpoint, p256dh, auth
           FROM web_push_subscriptions
           WHERE user_id = ?
           ORDER BY created_at DESC, id DESC
           LIMIT 1"#,
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;

    Ok(row.map(|row| WebPushSubscription {
        endpoint: row.get::<String, _>(0),
        keys: WebPushKeys {
            p256dh: row.get::<String, _>(1),
            auth: row.get::<String, _>(2),
        },
    }))
}

pub async fn cleanup_logs(
    db: &SqlitePool,
    retention_days: i64,
    max_rows: i64,
) -> anyhow::Result<()> {
    if retention_days > 0 {
        let cutoff = OffsetDateTime::now_utc()
            .saturating_sub(time::Duration::days(retention_days))
            .format(&Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());
        sqlx::query("DELETE FROM event_logs WHERE ts < ?")
            .bind(cutoff)
            .execute(db)
            .await?;
    }
    if max_rows > 0 {
        // Delete oldest rows beyond max_rows.
        sqlx::query(
            r#"
DELETE FROM event_logs
WHERE id IN (
  SELECT id FROM event_logs
  ORDER BY ts DESC, id DESC
  LIMIT -1 OFFSET ?
)"#,
        )
        .bind(max_rows)
        .execute(db)
        .await?;
    }
    Ok(())
}

pub async fn cleanup_ops(db: &SqlitePool, retention_days: i64) -> anyhow::Result<()> {
    if retention_days <= 0 {
        return Ok(());
    }

    let cutoff = OffsetDateTime::now_utc()
        .saturating_sub(time::Duration::days(retention_days))
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());

    sqlx::query("DELETE FROM ops_events WHERE ts < ?")
        .bind(&cutoff)
        .execute(db)
        .await?;
    sqlx::query("DELETE FROM ops_notify_runs WHERE ts < ?")
        .bind(&cutoff)
        .execute(db)
        .await?;
    sqlx::query(
        r#"
DELETE FROM ops_task_runs
WHERE (
  ended_at IS NOT NULL AND ended_at < ?
) OR (
  ended_at IS NULL AND started_at < ?
)
"#,
    )
    .bind(&cutoff)
    .bind(&cutoff)
    .execute(db)
    .await?;

    Ok(())
}

pub async fn cleanup_inventory_samples_1m(
    db: &SqlitePool,
    retention_days: i64,
) -> anyhow::Result<()> {
    if retention_days <= 0 {
        return Ok(());
    }
    let cutoff = floor_to_minute_utc(
        OffsetDateTime::now_utc().saturating_sub(time::Duration::days(retention_days)),
    )
    .format(&Rfc3339)
    .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());

    sqlx::query("DELETE FROM inventory_samples_1m WHERE ts_minute < ?")
        .bind(cutoff)
        .execute(db)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    #[test]
    fn floors_rfc3339_to_minute_utc() {
        let ts = "2026-01-21T12:34:56Z";
        let floored = floor_rfc3339_to_minute_utc(ts).unwrap();
        assert_eq!(floored, "2026-01-21T12:34:00Z");
    }

    #[test]
    fn notification_record_timestamps_use_fixed_width_utc() {
        let ts = OffsetDateTime::from_unix_timestamp_nanos(1_763_223_500_812_610_000).unwrap();
        assert_eq!(
            format_notification_record_rfc3339(ts),
            "2025-11-15T16:18:20.812610000Z"
        );
    }

    #[test]
    fn shared_timestamps_keep_legacy_rfc3339_format() {
        let ts = OffsetDateTime::from_unix_timestamp_nanos(1_763_223_500_812_610_000).unwrap();
        assert_eq!(format_rfc3339(ts), "2025-11-15T16:18:20.81261Z");
    }

    #[tokio::test]
    async fn country_scope_partition_exists_when_country_has_explicit_regions() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        init_db(&db).await.unwrap();

        replace_catalog_topology(
            &db,
            "https://example.invalid/cart",
            &[crate::models::Country {
                id: "7".to_string(),
                name: "Japan".to_string(),
            }],
            &[crate::models::Region {
                id: "40".to_string(),
                country_id: "7".to_string(),
                name: "Tokyo".to_string(),
                location_name: Some("JP-East".to_string()),
            }],
        )
        .await
        .unwrap();

        upsert_catalog_configs(
            &db,
            &[crate::upstream::ConfigBase {
                id: "cfg-1".to_string(),
                country_id: "7".to_string(),
                region_id: Some("40".to_string()),
                name: "JP test".to_string(),
                specs: Vec::new(),
                price: crate::models::Money {
                    amount: 39.0,
                    currency: "CNY".to_string(),
                    period: "month".to_string(),
                },
                inventory: crate::models::Inventory {
                    status: "in_stock".to_string(),
                    quantity: 1,
                    checked_at: "2026-01-21T12:34:56Z".to_string(),
                },
                digest: "digest-1".to_string(),
                monitor_supported: true,
                source_pid: None,
                source_fid: Some("7".to_string()),
                source_gid: Some("40".to_string()),
            }],
        )
        .await
        .unwrap();

        assert!(catalog_partition_exists(&db, "7", None).await.unwrap());
        let saved = set_monitoring_partition_enabled(&db, "u_1", "7", None, true)
            .await
            .unwrap();
        assert_eq!(saved.country_id, "7");
        assert_eq!(saved.region_id, None);
    }

    #[tokio::test]
    async fn partition_exists_rejects_removed_topology_even_with_history() {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        init_db(&db).await.unwrap();

        replace_catalog_topology(
            &db,
            "https://example.invalid/cart",
            &[crate::models::Country {
                id: "7".to_string(),
                name: "Japan".to_string(),
            }],
            &[crate::models::Region {
                id: "40".to_string(),
                country_id: "7".to_string(),
                name: "Tokyo".to_string(),
                location_name: Some("JP-East".to_string()),
            }],
        )
        .await
        .unwrap();

        upsert_catalog_configs(
            &db,
            &[crate::upstream::ConfigBase {
                id: "cfg-1".to_string(),
                country_id: "7".to_string(),
                region_id: Some("40".to_string()),
                name: "JP test".to_string(),
                specs: Vec::new(),
                price: crate::models::Money {
                    amount: 39.0,
                    currency: "CNY".to_string(),
                    period: "month".to_string(),
                },
                inventory: crate::models::Inventory {
                    status: "in_stock".to_string(),
                    quantity: 1,
                    checked_at: "2026-01-21T12:34:56Z".to_string(),
                },
                digest: "digest-1".to_string(),
                monitor_supported: true,
                source_pid: None,
                source_fid: Some("7".to_string()),
                source_gid: Some("40".to_string()),
            }],
        )
        .await
        .unwrap();

        replace_catalog_topology(&db, "https://example.invalid/cart", &[], &[])
            .await
            .unwrap();
        retire_catalog_targets(&db, &[("7".to_string(), Some("40".to_string()))])
            .await
            .unwrap();

        assert!(!catalog_partition_exists(&db, "7", None).await.unwrap());
        assert!(!catalog_partition_exists(&db, "7", Some("40"))
            .await
            .unwrap());
    }
}
