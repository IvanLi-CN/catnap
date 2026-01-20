use crate::config::RuntimeConfig;
use crate::models::*;
use sqlx::{Row, SqlitePool};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SettingsRow {
    pub poll_interval_minutes: i64,
    pub poll_jitter_pct: f64,
    pub site_base_url: Option<String>,

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
  source_pid TEXT NULL,
  source_fid TEXT NULL,
  source_gid TEXT NULL
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

CREATE INDEX IF NOT EXISTS idx_event_logs_user_ts ON event_logs (user_id, ts DESC, id DESC);
CREATE INDEX IF NOT EXISTS idx_event_logs_ts ON event_logs (ts DESC, id DESC);
"#,
    )
    .execute(db)
    .await?;
    Ok(())
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| {
            // Should not happen; keep response stable.
            "1970-01-01T00:00:00Z".to_string()
        })
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
            telegram_enabled,
            telegram_bot_token,
            telegram_target,
            web_push_enabled,
            created_at,
            updated_at
        ) VALUES (?, ?, ?, NULL, 0, NULL, NULL, 0, ?, ?)"#,
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
        telegram_enabled: row.get::<i64, _>(3) != 0,
        telegram_bot_token: row.get::<Option<String>, _>(4),
        telegram_target: row.get::<Option<String>, _>(5),
        web_push_enabled: row.get::<i64, _>(6) != 0,
        created_at: row.get::<String, _>(7),
        updated_at: row.get::<String, _>(8),
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

    sqlx::query(
        r#"UPDATE settings SET
            poll_interval_minutes = ?,
            poll_jitter_pct = ?,
            site_base_url = ?,
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
  config_digest, source_pid, source_fid, source_gid
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
        .bind(c.source_pid.as_deref())
        .bind(c.source_fid.as_deref())
        .bind(c.source_gid.as_deref())
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
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
            let mut parts = c.splitn(2, ':');
            Some((parts.next()?.to_string(), parts.next()?.to_string()))
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
