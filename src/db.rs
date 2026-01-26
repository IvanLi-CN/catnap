use crate::config::RuntimeConfig;
use crate::models::*;
use sqlx::{Row, SqlitePool};
use time::{format_description::well_known::Rfc3339, OffsetDateTime, UtcOffset};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SettingsRow {
    pub poll_interval_minutes: i64,
    pub poll_jitter_pct: f64,
    pub site_base_url: Option<String>,

    pub catalog_refresh_auto_interval_hours: Option<i64>,
    pub monitoring_events_listed_enabled: bool,
    pub monitoring_events_delisted_enabled: bool,

    pub telegram_enabled: bool,
    pub telegram_bot_token: Option<String>,
    pub telegram_target: Option<String>,

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
                auto_interval_hours: self.catalog_refresh_auto_interval_hours,
            },
            monitoring_events: SettingsMonitoringEventsView {
                listed_enabled: self.monitoring_events_listed_enabled,
                delisted_enabled: self.monitoring_events_delisted_enabled,
            },
            notifications: SettingsNotificationsView {
                telegram: TelegramSettingsView {
                    enabled: self.telegram_enabled,
                    configured: self
                        .telegram_bot_token
                        .as_ref()
                        .is_some_and(|v| !v.trim().is_empty())
                        && self
                            .telegram_target
                            .as_ref()
                            .is_some_and(|v| !v.trim().is_empty()),
                    target: self
                        .telegram_target
                        .clone()
                        .filter(|v| !v.trim().is_empty()),
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

CREATE TABLE IF NOT EXISTS settings (
  user_id TEXT PRIMARY KEY,
  poll_interval_minutes INTEGER NOT NULL,
  poll_jitter_pct REAL NOT NULL,
  site_base_url TEXT NULL,
  catalog_refresh_auto_interval_hours INTEGER NULL,
  monitoring_events_listed_enabled INTEGER NOT NULL DEFAULT 0,
  monitoring_events_delisted_enabled INTEGER NOT NULL DEFAULT 0,
  telegram_enabled INTEGER NOT NULL,
  telegram_bot_token TEXT NULL,
  telegram_target TEXT NULL,
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
CREATE INDEX IF NOT EXISTS idx_inventory_samples_1m_ts ON inventory_samples_1m (ts_minute);
CREATE INDEX IF NOT EXISTS idx_catalog_url_cache_last_success_at ON catalog_url_cache (last_success_at DESC, url_key);

CREATE INDEX IF NOT EXISTS idx_ops_events_ts ON ops_events (ts DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_ops_task_runs_ended_at ON ops_task_runs (ended_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_ops_task_runs_key ON ops_task_runs (fid, gid, ended_at DESC);
CREATE INDEX IF NOT EXISTS idx_ops_notify_runs_task_run_id ON ops_notify_runs (task_run_id);
CREATE INDEX IF NOT EXISTS idx_ops_notify_runs_channel_ts ON ops_notify_runs (channel, ts DESC);
"#,
    )
    .execute(db)
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
        "monitoring_events_delisted_enabled",
        "INTEGER NOT NULL DEFAULT 0",
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
    Ok(())
}

async fn add_column_if_missing(
    db: &SqlitePool,
    table: &str,
    column: &str,
    column_def: &str,
) -> anyhow::Result<()> {
    let sql = format!("ALTER TABLE {table} ADD COLUMN {column} {column_def}");
    match sqlx::query(&sql).execute(db).await {
        Ok(_) => Ok(()),
        Err(err) => {
            // SQLite emits: "duplicate column name: <col>"
            let msg = err.to_string();
            if msg.to_lowercase().contains("duplicate column name") {
                Ok(())
            } else {
                Err(err.into())
            }
        }
    }
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| {
            // Should not happen; keep response stable.
            "1970-01-01T00:00:00Z".to_string()
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
            monitoring_events_delisted_enabled,
            telegram_enabled,
            telegram_bot_token,
            telegram_target,
            web_push_enabled,
            created_at,
            updated_at
        ) VALUES (?, ?, ?, NULL, 6, 0, 0, 0, NULL, NULL, 0, ?, ?)"#,
    )
    .bind(user_id)
    .bind(cfg.default_poll_interval_minutes)
    .bind(cfg.default_poll_jitter_pct)
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
            monitoring_events_listed_enabled,
            monitoring_events_delisted_enabled,
            telegram_enabled,
            telegram_bot_token,
            telegram_target,
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
        monitoring_events_listed_enabled: row.get::<i64, _>(4) != 0,
        monitoring_events_delisted_enabled: row.get::<i64, _>(5) != 0,
        telegram_enabled: row.get::<i64, _>(6) != 0,
        telegram_bot_token: row.get::<Option<String>, _>(7),
        telegram_target: row.get::<Option<String>, _>(8),
        web_push_enabled: row.get::<i64, _>(9) != 0,
        created_at: row.get::<String, _>(10),
        updated_at: row.get::<String, _>(11),
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
    let existing_target = existing.telegram_target;
    let existing_auto_interval_hours = existing.catalog_refresh_auto_interval_hours;
    let existing_listed_enabled = existing.monitoring_events_listed_enabled;
    let existing_delisted_enabled = existing.monitoring_events_delisted_enabled;

    let telegram_bot_token = req
        .notifications
        .telegram
        .bot_token
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .or(existing_bot_token);
    let telegram_target = req
        .notifications
        .telegram
        .target
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .or(existing_target);

    let auto_interval_hours = req
        .catalog_refresh
        .map(|v| v.auto_interval_hours)
        .unwrap_or(existing_auto_interval_hours);
    let listed_enabled = req
        .monitoring_events
        .as_ref()
        .map(|v| v.listed_enabled)
        .unwrap_or(existing_listed_enabled);
    let delisted_enabled = req
        .monitoring_events
        .as_ref()
        .map(|v| v.delisted_enabled)
        .unwrap_or(existing_delisted_enabled);

    sqlx::query(
        r#"UPDATE settings SET
            poll_interval_minutes = ?,
            poll_jitter_pct = ?,
            site_base_url = ?,
            catalog_refresh_auto_interval_hours = ?,
            monitoring_events_listed_enabled = ?,
            monitoring_events_delisted_enabled = ?,
            telegram_enabled = ?,
            telegram_bot_token = ?,
            telegram_target = ?,
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
    .bind(if listed_enabled { 1 } else { 0 })
    .bind(if delisted_enabled { 1 } else { 0 })
    .bind(if req.notifications.telegram.enabled {
        1
    } else {
        0
    })
    .bind(telegram_bot_token)
    .bind(telegram_target)
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

fn monitor_supported_for_country(country_id: &str) -> bool {
    country_id.trim() != "2"
}

fn config_view_from_row(row: &sqlx::sqlite::SqliteRow) -> anyhow::Result<ConfigView> {
    let specs_json = row.get::<String, _>("specs_json");
    let specs: Vec<Spec> = serde_json::from_str(&specs_json).unwrap_or_default();

    let country_id = row.get::<String, _>("country_id");
    let region_id = row.get::<Option<String>, _>("region_id");
    let lifecycle_state = row.get::<String, _>("lifecycle_state").trim().to_string();
    let listed_at = row.get::<String, _>("lifecycle_listed_at");
    let delisted_at = row.get::<Option<String>, _>("lifecycle_delisted_at");

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
        },
        monitor_supported: monitor_supported_for_country(&country_id),
        monitor_enabled: row.get::<i64, _>("monitor_enabled") != 0,
    })
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
  COALESCE(m.enabled, 0) AS monitor_enabled
FROM catalog_configs c
LEFT JOIN monitoring_configs m
  ON m.user_id = ? AND m.config_id = c.id
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

    let mut q = sqlx::query(&sql).bind(user_id);
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
  COALESCE(m.enabled, 0) AS monitor_enabled
FROM catalog_configs c
JOIN monitoring_configs m
  ON m.user_id = ? AND m.config_id = c.id AND m.enabled = 1
ORDER BY c.country_id ASC, c.region_id ASC, c.price_amount ASC, c.id ASC
"#,
    )
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
  COALESCE(m.enabled, 0) AS monitor_enabled
FROM catalog_configs c
LEFT JOIN monitoring_configs m
  ON m.user_id = ? AND m.config_id = c.id
WHERE c.lifecycle_listed_at >= ?
ORDER BY c.lifecycle_listed_at DESC, c.id DESC
LIMIT 200
"#,
    )
    .bind(user_id)
    .bind(cutoff)
    .fetch_all(db)
    .await?;
    rows.iter().map(config_view_from_row).collect()
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
    pub delisted_ids: Vec<String>,
    pub fetched_at: String,
}

pub async fn apply_catalog_url_fetch_success(
    db: &SqlitePool,
    fid: &str,
    gid: Option<&str>,
    url_key: &str,
    url: &str,
    mut configs: Vec<crate::upstream::ConfigBase>,
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

    // A parse that yields an empty list is ambiguous: it could mean the upstream cart truly has no
    // items, or it could be an upstream HTML change/error page that our parser didn't catch.
    // Treat it as a failure when we have previously active IDs to avoid incorrect mass-delisting.
    if configs.is_empty() && !prev_ids.is_empty() {
        anyhow::bail!(
            "refusing to apply empty catalog config list for {url_key} (would delist {} ids)",
            prev_ids.len()
        );
    }

    let fetched_ids: std::collections::HashSet<String> =
        configs.iter().map(|c| c.id.clone()).collect();
    let listed_ids = fetched_ids
        .difference(&prev_ids)
        .cloned()
        .collect::<Vec<_>>();
    let delisted_ids = prev_ids
        .difference(&fetched_ids)
        .cloned()
        .collect::<Vec<_>>();

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
  source_pid, source_fid, source_gid
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'active', ?, NULL, ?, ?, ?, ?)
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
  source_pid = excluded.source_pid,
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

    if !delisted_ids.is_empty() {
        // Mark configs as delisted (one success-miss = delist).
        let placeholders = std::iter::repeat_n("?", delisted_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            r#"
UPDATE catalog_configs
SET lifecycle_state = 'delisted',
    lifecycle_delisted_at = ?
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

    tx.commit().await?;

    Ok(ApplyCatalogUrlResult {
        listed_ids,
        delisted_ids,
        fetched_at,
    })
}

pub async fn upsert_catalog_configs(
    db: &SqlitePool,
    configs: &[crate::upstream::ConfigBase],
) -> anyhow::Result<()> {
    let mut tx = db.begin().await?;
    for c in configs {
        sqlx::query(
            r#"
INSERT INTO catalog_configs (
  id, country_id, region_id, name, specs_json,
  price_amount, price_currency, price_period,
  inventory_status, inventory_quantity, checked_at,
  config_digest,
  lifecycle_state, lifecycle_listed_at, lifecycle_delisted_at, lifecycle_last_seen_at,
  source_pid, source_fid, source_gid
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'active', ?, NULL, ?, ?, ?, ?)
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
  source_pid = excluded.source_pid,
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

    #[test]
    fn floors_rfc3339_to_minute_utc() {
        let ts = "2026-01-21T12:34:56Z";
        let floored = floor_rfc3339_to_minute_utc(ts).unwrap();
        assert_eq!(floored, "2026-01-21T12:34:00Z");
    }
}
