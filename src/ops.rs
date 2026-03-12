use crate::config::RuntimeConfig;
use crate::models::Money;
use crate::notification_content::{
    self, ConfigLifecycleNotificationKind, TopologyNotificationKind,
};
use crate::notifications;
use crate::upstream::{catalog_region_key, CatalogSnapshot, UpstreamClient};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::sync::{broadcast, oneshot, Mutex, Notify, RwLock};
use tracing::warn;

const POLLER_FRESHNESS_WINDOW_SECONDS: i64 = 45;
const DISCOVERY_FRESHNESS_WINDOW_SECONDS: i64 = 150;
const MANUAL_REFRESH_FRESHNESS_WINDOW_SECONDS: i64 = 5 * 60;
const AUTO_REFRESH_FRESHNESS_WINDOW_SECONDS: i64 = 5 * 60;

fn reason_freshness_window_seconds(reason: &str) -> Option<i64> {
    match reason {
        "poller_due" => Some(POLLER_FRESHNESS_WINDOW_SECONDS),
        "discovery_due" => Some(DISCOVERY_FRESHNESS_WINDOW_SECONDS),
        "manual_refresh" => Some(MANUAL_REFRESH_FRESHNESS_WINDOW_SECONDS),
        "auto_refresh" => Some(AUTO_REFRESH_FRESHNESS_WINDOW_SECONDS),
        _ => None,
    }
}

fn task_freshness_window_seconds(reason_counts: &HashMap<String, i64>) -> Option<i64> {
    reason_counts
        .keys()
        .filter_map(|reason| reason_freshness_window_seconds(reason))
        .max()
}

fn should_emit_lifecycle_notify(reason_counts: &HashMap<String, i64>) -> bool {
    reason_counts.keys().any(|reason| {
        matches!(
            reason.as_str(),
            "manual_refresh" | "auto_refresh" | "poller_due" | "discovery_due"
        )
    })
}

fn format_pending_stock_message(
    name: &str,
    id: &str,
    qty: i64,
    price: &Money,
    monitoring_url: Option<&str>,
) -> String {
    let mut message = format!(
        "{name} ({id}) 已上架，但当前库存为 {qty}，暂不发送上架通知。{}",
        notification_content::format_money(price)
    );
    if let Some(url) = monitoring_url
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        message.push(' ');
        message.push_str(url.trim_end_matches('/'));
        message.push_str("/monitoring");
    }
    message
}

#[derive(Debug, Clone)]
struct LifecycleNotifyState {
    reason_counts: HashMap<String, i64>,
    poller_waiter_user_ids: HashSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpsRange {
    H24,
    D7,
    D30,
}

impl OpsRange {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "24h" => Some(Self::H24),
            "7d" => Some(Self::D7),
            "30d" => Some(Self::D30),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::H24 => "24h",
            Self::D7 => "7d",
            Self::D30 => "30d",
        }
    }

    pub fn duration(self) -> time::Duration {
        match self {
            Self::H24 => time::Duration::hours(24),
            Self::D7 => time::Duration::days(7),
            Self::D30 => time::Duration::days(30),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TaskKey {
    fid: String,
    gid: Option<String>,
}

impl TaskKey {
    fn to_view(&self) -> OpsTaskKeyView {
        OpsTaskKeyView {
            fid: self.fid.clone(),
            gid: self.gid.clone(),
        }
    }
}

#[derive(Debug)]
struct TaskEntry {
    state: TaskEntryState,
    enqueued_at: String,
    reason_counts: HashMap<String, i64>,
    force_fetch: bool,
    joiners: Vec<oneshot::Sender<OpsRunOutcome>>,
    poller_waiter_user_ids: HashSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TaskEntryState {
    Pending,
    Running { run_id: i64, started_at: String },
}

#[derive(Debug, Clone)]
struct WorkerRuntime {
    worker_id: String,
    state: WorkerState,
    task: Option<TaskKey>,
    started_at: Option<String>,
    last_error: Option<WorkerError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkerState {
    Idle,
    Running,
    Error,
}

#[derive(Debug, Clone)]
struct WorkerError {
    ts: String,
    message: String,
}

#[derive(Debug)]
struct RuntimeState {
    deduped: i64,
    pending: VecDeque<TaskKey>,
    tasks: HashMap<TaskKey, TaskEntry>,
    workers: Vec<WorkerRuntime>,
}

#[derive(Debug, Clone)]
pub struct StoredOpsEvent {
    pub id: i64,
    pub event: String,
    pub data_json: String,
    pub ts: String,
}

#[derive(Clone)]
pub struct OpsManager {
    inner: Arc<Inner>,
}

struct Inner {
    cfg: RuntimeConfig,
    db: SqlitePool,
    catalog: Arc<RwLock<CatalogSnapshot>>,
    tx: broadcast::Sender<StoredOpsEvent>,
    publish_lock: Mutex<()>,
    state: Mutex<RuntimeState>,
    notify: Notify,
}

#[derive(Debug, Clone)]
struct NotificationDeliveryTarget {
    user_id: String,
    site_base_url: Option<String>,
    tg_enabled: bool,
    tg_bot_token: Option<String>,
    tg_target: Option<String>,
    wp_enabled: bool,
}

impl NotificationDeliveryTarget {
    fn from_row(row: sqlx::sqlite::SqliteRow) -> Self {
        Self {
            user_id: row.get::<String, _>(0),
            site_base_url: row.get::<Option<String>, _>(1),
            tg_enabled: row.get::<i64, _>(2) != 0,
            tg_bot_token: row.get::<Option<String>, _>(3),
            tg_target: row.get::<Option<String>, _>(4),
            wp_enabled: row.get::<i64, _>(5) != 0,
        }
    }
}

#[derive(Debug, Clone)]
struct ConfigLifecycleRecord {
    id: String,
    name: String,
    price: Money,
    quantity: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountryTopologyChange {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionTopologyChange {
    pub country_id: String,
    pub country_name: String,
    pub region_id: String,
    pub region_name: String,
}

async fn load_partition_label(
    db: &SqlitePool,
    country_id: &str,
    region_id: Option<&str>,
) -> anyhow::Result<Option<String>> {
    let country_row = sqlx::query("SELECT name FROM catalog_countries WHERE id = ?")
        .bind(country_id)
        .fetch_optional(db)
        .await?;
    let Some(country_row) = country_row else {
        return Ok(None);
    };
    let country_name = country_row.get::<String, _>(0);
    let Some(region_id) = region_id else {
        return Ok(Some(country_name));
    };
    let region_row =
        sqlx::query("SELECT name FROM catalog_regions WHERE country_id = ? AND id = ?")
            .bind(country_id)
            .bind(region_id)
            .fetch_optional(db)
            .await?;
    let Some(region_row) = region_row else {
        return Ok(Some(country_name));
    };
    let region_name = region_row.get::<String, _>(0);
    Ok(Some(format!("{country_name} / {region_name}")))
}

async fn load_config_lifecycle_records(
    db: &SqlitePool,
    ids: &[String],
) -> anyhow::Result<HashMap<String, ConfigLifecycleRecord>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let placeholders = std::iter::repeat_n("?", ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        r#"
SELECT id, name, price_amount, price_currency, price_period, inventory_quantity
FROM catalog_configs
WHERE id IN ({placeholders})
"#
    );
    let mut q = sqlx::query(&sql);
    for id in ids {
        q = q.bind(id);
    }
    let rows = q.fetch_all(db).await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let record = ConfigLifecycleRecord {
                id: row.get::<String, _>(0),
                name: row.get::<String, _>(1),
                price: Money {
                    amount: row.get::<f64, _>(2),
                    currency: row.get::<String, _>(3),
                    period: row.get::<String, _>(4),
                },
                quantity: row.get::<i64, _>(5),
            };
            (record.id.clone(), record)
        })
        .collect())
}

async fn load_country_catalog_summary(
    db: &SqlitePool,
    country_id: &str,
) -> anyhow::Result<(Vec<notification_content::CatalogSummaryItem>, usize)> {
    let total_count = sqlx::query(
        r#"
SELECT COUNT(*)
FROM catalog_configs
WHERE country_id = ?
  AND lifecycle_state = 'active'
"#,
    )
    .bind(country_id)
    .fetch_one(db)
    .await?
    .get::<i64, _>(0)
    .max(0) as usize;

    let rows = sqlx::query(
        r#"
SELECT name, price_amount, price_currency, price_period
FROM catalog_configs
WHERE country_id = ?
  AND lifecycle_state = 'active'
ORDER BY price_amount ASC, id ASC
LIMIT 10
"#,
    )
    .bind(country_id)
    .fetch_all(db)
    .await?;

    let items = rows
        .into_iter()
        .map(|row| notification_content::CatalogSummaryItem {
            name: row.get::<String, _>(0),
            price: Money {
                amount: row.get::<f64, _>(1),
                currency: row.get::<String, _>(2),
                period: row.get::<String, _>(3),
            },
        })
        .collect();

    Ok((items, total_count))
}

async fn load_region_catalog_summary(
    db: &SqlitePool,
    country_id: &str,
    region_id: &str,
) -> anyhow::Result<(Vec<notification_content::CatalogSummaryItem>, usize)> {
    let total_count = sqlx::query(
        r#"
SELECT COUNT(*)
FROM catalog_configs
WHERE country_id = ?
  AND region_id = ?
  AND lifecycle_state = 'active'
"#,
    )
    .bind(country_id)
    .bind(region_id)
    .fetch_one(db)
    .await?
    .get::<i64, _>(0)
    .max(0) as usize;

    let rows = sqlx::query(
        r#"
SELECT name, price_amount, price_currency, price_period
FROM catalog_configs
WHERE country_id = ?
  AND region_id = ?
  AND lifecycle_state = 'active'
ORDER BY price_amount ASC, id ASC
LIMIT 10
"#,
    )
    .bind(country_id)
    .bind(region_id)
    .fetch_all(db)
    .await?;

    let items = rows
        .into_iter()
        .map(|row| notification_content::CatalogSummaryItem {
            name: row.get::<String, _>(0),
            price: Money {
                amount: row.get::<f64, _>(1),
                currency: row.get::<String, _>(2),
                period: row.get::<String, _>(3),
            },
        })
        .collect();

    Ok((items, total_count))
}

async fn deliver_outbound_notification(
    manager: &OpsManager,
    run_id: Option<i64>,
    target: &NotificationDeliveryTarget,
    scope: &str,
    msg: &str,
    meta: serde_json::Value,
    notification: &notification_content::OutboundNotification,
) -> anyhow::Result<()> {
    let notify_run_id = run_id.unwrap_or(0);
    let _ = crate::db::insert_log(
        &manager.inner.db,
        Some(&target.user_id),
        "info",
        scope,
        msg,
        Some(meta.clone()),
    )
    .await;
    let _ = manager
        .log(
            "info",
            scope,
            msg,
            Some(serde_json::json!({
                "runId": run_id,
                "userId": target.user_id,
                "meta": meta,
            })),
        )
        .await;

    if target.tg_enabled {
        match (target.tg_bot_token.as_deref(), target.tg_target.as_deref()) {
            (Some(token), Some(target_chat)) => match notifications::send_telegram(
                &manager.inner.cfg.telegram_api_base_url,
                token,
                target_chat,
                &notification.telegram_text,
            )
            .await
            {
                Ok(_) => {
                    let _ = manager
                        .record_notify(notify_run_id, "telegram", "success", None)
                        .await;
                }
                Err(err) => {
                    let err_msg = err.to_string();
                    let _ = manager
                        .record_notify(notify_run_id, "telegram", "error", Some(&err_msg))
                        .await;
                    let _ = crate::db::insert_log(
                        &manager.inner.db,
                        Some(&target.user_id),
                        "warn",
                        "notify.telegram",
                        "telegram send failed",
                        Some(serde_json::json!({ "error": err.to_string() })),
                    )
                    .await;
                }
            },
            _ => {
                let _ = manager
                    .record_notify(
                        notify_run_id,
                        "telegram",
                        "skipped",
                        Some("missing telegram config"),
                    )
                    .await;
            }
        }
    }

    if target.wp_enabled {
        match crate::db::get_latest_web_push_subscription(&manager.inner.db, &target.user_id).await
        {
            Ok(Some(sub)) => match notifications::send_web_push(
                &manager.inner.cfg,
                &sub,
                &notification.web_push_title,
                &notification.web_push_body,
                &notification.web_push_url,
            )
            .await
            {
                Ok(_) => {
                    let _ = manager
                        .record_notify(notify_run_id, "webPush", "success", None)
                        .await;
                }
                Err(err) => {
                    let err_msg = err.to_string();
                    let _ = manager
                        .record_notify(notify_run_id, "webPush", "error", Some(&err_msg))
                        .await;
                }
            },
            Ok(None) => {
                let _ = manager
                    .record_notify(
                        notify_run_id,
                        "webPush",
                        "skipped",
                        Some("missing web push subscription"),
                    )
                    .await;
            }
            Err(err) => {
                let err_msg = err.to_string();
                let _ = manager
                    .record_notify(notify_run_id, "webPush", "error", Some(&err_msg))
                    .await;
            }
        }
    }

    Ok(())
}

async fn load_enabled_monitoring_ids(
    db: &SqlitePool,
    user_id: &str,
    ids: &[String],
) -> anyhow::Result<HashSet<String>> {
    if ids.is_empty() {
        return Ok(HashSet::new());
    }
    let placeholders = std::iter::repeat_n("?", ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        r#"
SELECT config_id
FROM monitoring_configs
WHERE user_id = ?
  AND enabled = 1
  AND config_id IN ({placeholders})
"#
    );
    let mut q = sqlx::query(&sql).bind(user_id);
    for id in ids {
        q = q.bind(id);
    }
    Ok(q.fetch_all(db)
        .await?
        .into_iter()
        .map(|row| row.get::<String, _>(0))
        .collect())
}

async fn load_monitored_delivery_targets(
    db: &SqlitePool,
    ids: &[String],
) -> anyhow::Result<Vec<NotificationDeliveryTarget>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = std::iter::repeat_n("?", ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        r#"
SELECT DISTINCT
  s.user_id,
  s.site_base_url,
  s.telegram_enabled,
  s.telegram_bot_token,
  s.telegram_target,
  s.web_push_enabled
FROM settings s
JOIN monitoring_configs m
  ON m.user_id = s.user_id
 AND m.enabled = 1
WHERE m.config_id IN ({placeholders})
"#
    );
    let mut q = sqlx::query(&sql);
    for id in ids {
        q = q.bind(id);
    }
    Ok(q.fetch_all(db)
        .await?
        .into_iter()
        .map(NotificationDeliveryTarget::from_row)
        .collect())
}

async fn deliver_monitoring_change_notification(
    manager: &OpsManager,
    run_id: i64,
    target: &NotificationDeliveryTarget,
    msg: &str,
    meta: serde_json::Value,
    notification: &notification_content::MonitoringChangeNotification,
) -> anyhow::Result<()> {
    let _ = crate::db::insert_log(
        &manager.inner.db,
        Some(&target.user_id),
        "info",
        "poll",
        msg,
        Some(meta.clone()),
    )
    .await;
    let _ = manager
        .log(
            "info",
            "poll.result",
            msg,
            Some(serde_json::json!({
                "runId": run_id,
                "userId": target.user_id,
                "meta": meta,
            })),
        )
        .await;

    if target.tg_enabled {
        match (target.tg_bot_token.as_deref(), target.tg_target.as_deref()) {
            (Some(token), Some(target_chat)) => match notifications::send_telegram(
                &manager.inner.cfg.telegram_api_base_url,
                token,
                target_chat,
                &notification.telegram_text,
            )
            .await
            {
                Ok(_) => {
                    let _ = manager
                        .record_notify(run_id, "telegram", "success", None)
                        .await;
                }
                Err(err) => {
                    let err_msg = err.to_string();
                    let _ = manager
                        .record_notify(run_id, "telegram", "error", Some(&err_msg))
                        .await;
                    let _ = crate::db::insert_log(
                        &manager.inner.db,
                        Some(&target.user_id),
                        "warn",
                        "notify.telegram",
                        "telegram send failed",
                        Some(serde_json::json!({ "error": err.to_string() })),
                    )
                    .await;
                }
            },
            _ => {
                let _ = manager
                    .record_notify(
                        run_id,
                        "telegram",
                        "skipped",
                        Some("missing telegram config"),
                    )
                    .await;
            }
        }
    }

    if target.wp_enabled {
        match crate::db::get_latest_web_push_subscription(&manager.inner.db, &target.user_id).await
        {
            Ok(Some(sub)) => match notifications::send_web_push(
                &manager.inner.cfg,
                &sub,
                &notification.web_push_title,
                &notification.web_push_body,
                &notification.web_push_url,
            )
            .await
            {
                Ok(_) => {
                    let _ = manager
                        .record_notify(run_id, "webPush", "success", None)
                        .await;
                }
                Err(err) => {
                    let err_msg = err.to_string();
                    let _ = manager
                        .record_notify(run_id, "webPush", "error", Some(&err_msg))
                        .await;
                }
            },
            Ok(None) => {
                let _ = manager
                    .record_notify(
                        run_id,
                        "webPush",
                        "skipped",
                        Some("missing web push subscription"),
                    )
                    .await;
            }
            Err(err) => {
                let err_msg = err.to_string();
                let _ = manager
                    .record_notify(run_id, "webPush", "error", Some(&err_msg))
                    .await;
            }
        }
    }

    Ok(())
}

#[derive(Debug, Clone)]
pub struct OpsRunOutcome {
    pub run_id: i64,
    pub ok: bool,
}

impl OpsManager {
    pub fn new(cfg: RuntimeConfig, db: SqlitePool, catalog: Arc<RwLock<CatalogSnapshot>>) -> Self {
        let (tx, _) = broadcast::channel(512);
        let concurrency = cfg.ops_worker_concurrency.max(1);
        let workers = (0..concurrency)
            .map(|i| WorkerRuntime {
                worker_id: format!("w{}", i + 1),
                state: WorkerState::Idle,
                task: None,
                started_at: None,
                last_error: None,
            })
            .collect::<Vec<_>>();
        Self {
            inner: Arc::new(Inner {
                cfg,
                db,
                catalog,
                tx,
                publish_lock: Mutex::new(()),
                state: Mutex::new(RuntimeState {
                    deduped: 0,
                    pending: VecDeque::new(),
                    tasks: HashMap::new(),
                    workers,
                }),
                notify: Notify::new(),
            }),
        }
    }

    pub fn start(&self) {
        let concurrency = self.inner.cfg.ops_worker_concurrency.max(1);
        for worker_idx in 0..concurrency {
            let this = self.clone();
            tokio::spawn(async move { this.worker_loop(worker_idx).await });
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<StoredOpsEvent> {
        self.inner.tx.subscribe()
    }

    pub async fn cursor_id(&self) -> anyhow::Result<i64> {
        let row = sqlx::query("SELECT COALESCE(MAX(id), 0) FROM ops_events")
            .fetch_one(&self.inner.db)
            .await?;
        Ok(row.get::<i64, _>(0))
    }

    pub async fn min_replay_id_since(&self, cutoff_ts: &str) -> anyhow::Result<Option<i64>> {
        let row = sqlx::query("SELECT MIN(id) FROM ops_events WHERE ts >= ?")
            .bind(cutoff_ts)
            .fetch_one(&self.inner.db)
            .await?;
        Ok(row.try_get::<i64, _>(0).ok())
    }

    pub async fn replay_since(
        &self,
        after_id: i64,
        cutoff_ts: &str,
    ) -> anyhow::Result<Vec<StoredOpsEvent>> {
        let rows = sqlx::query(
            r#"
SELECT id, ts, event, data_json
FROM ops_events
WHERE id > ?
  AND ts >= ?
ORDER BY id ASC
LIMIT 2000
"#,
        )
        .bind(after_id)
        .bind(cutoff_ts)
        .fetch_all(&self.inner.db)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| StoredOpsEvent {
                id: r.get::<i64, _>(0),
                ts: r.get::<String, _>(1),
                event: r.get::<String, _>(2),
                data_json: r.get::<String, _>(3),
            })
            .collect())
    }

    pub async fn enqueue_and_wait(
        &self,
        fid: &str,
        gid: Option<&str>,
        reason: &str,
    ) -> anyhow::Result<OpsRunOutcome> {
        let rx = self.enqueue(fid, gid, reason, false, None).await?;
        rx.await.map_err(|_| anyhow::anyhow!("ops task canceled"))
    }

    pub async fn enqueue_and_wait_for_poller(
        &self,
        fid: &str,
        gid: Option<&str>,
        user_id: &str,
    ) -> anyhow::Result<OpsRunOutcome> {
        let rx = self
            .enqueue(fid, gid, "poller_due", false, Some(user_id))
            .await?;
        rx.await.map_err(|_| anyhow::anyhow!("ops task canceled"))
    }

    pub async fn enqueue_and_wait_force_fetch(
        &self,
        fid: &str,
        gid: Option<&str>,
        reason: &str,
    ) -> anyhow::Result<OpsRunOutcome> {
        let rx = self.enqueue(fid, gid, reason, true, None).await?;
        rx.await.map_err(|_| anyhow::anyhow!("ops task canceled"))
    }

    pub async fn enqueue_background(
        &self,
        fid: &str,
        gid: Option<&str>,
        reason: &str,
    ) -> anyhow::Result<()> {
        std::mem::drop(self.enqueue(fid, gid, reason, false, None).await?);
        Ok(())
    }

    pub async fn log(
        &self,
        level: &str,
        scope: &str,
        message: &str,
        meta: Option<serde_json::Value>,
    ) -> anyhow::Result<i64> {
        let ts = now_rfc3339();
        let payload_ts = ts.clone();
        let payload = serde_json::json!({
            "ts": payload_ts,
            "level": level,
            "scope": scope,
            "message": message,
            "meta": meta,
        });
        self.publish_event_with_ts("ops.log", &ts, payload).await
    }

    pub async fn record_notify(
        &self,
        task_run_id: i64,
        channel: &str,
        result: &str,
        message: Option<&str>,
    ) -> anyhow::Result<()> {
        let ts = now_rfc3339();
        sqlx::query(
            r#"
INSERT INTO ops_notify_runs (task_run_id, ts, channel, result, error_message)
VALUES (?, ?, ?, ?, ?)
"#,
        )
        .bind(task_run_id)
        .bind(&ts)
        .bind(channel)
        .bind(result)
        .bind(message)
        .execute(&self.inner.db)
        .await?;

        let _ = self
            .publish_event(
                "ops.notify",
                serde_json::json!({
                    "runId": task_run_id,
                    "channel": channel,
                    "result": result,
                    "message": message,
                }),
            )
            .await;

        let log_level = if result == "success" { "info" } else { "warn" };
        let log_msg = match (result, message) {
            ("success", Some(m)) => format!("notify {channel}: success ({m})"),
            ("success", None) => format!("notify {channel}: success"),
            ("skipped", Some(m)) => format!("notify {channel}: skipped ({m})"),
            ("skipped", None) => format!("notify {channel}: skipped"),
            (_, Some(m)) => format!("notify {channel}: error ({m})"),
            _ => format!("notify {channel}: error"),
        };
        let _ = self
            .log(
                log_level,
                &format!("notify.{channel}"),
                &log_msg,
                Some(serde_json::json!({
                    "runId": task_run_id,
                    "channel": channel,
                    "result": result,
                })),
            )
            .await;

        Ok(())
    }

    pub async fn snapshot(
        &self,
        range: OpsRange,
        log_limit: Option<i64>,
        task_limit: Option<i64>,
    ) -> anyhow::Result<OpsStateSnapshot> {
        let now = OffsetDateTime::now_utc();
        let server_time = now_rfc3339();
        let replay_window_seconds = self.inner.cfg.ops_sse_replay_window_seconds;

        let log_limit = log_limit
            .unwrap_or(self.inner.cfg.ops_log_tail_limit_default)
            .clamp(1, 500);
        let task_limit = task_limit
            .unwrap_or(self.inner.cfg.ops_queue_task_limit_default)
            .clamp(1, 500);

        let (queue, workers, tasks) = {
            let st = self.inner.state.lock().await;
            let pending = st
                .tasks
                .values()
                .filter(|t| matches!(t.state, TaskEntryState::Pending))
                .count() as i64;
            let running = st
                .tasks
                .values()
                .filter(|t| matches!(t.state, TaskEntryState::Running { .. }))
                .count() as i64;
            let oldest_wait_seconds = st
                .tasks
                .values()
                .filter_map(|task| OffsetDateTime::parse(&task.enqueued_at, &Rfc3339).ok())
                .map(|enqueued_at| (now - enqueued_at).whole_seconds().max(0))
                .max();
            let mut queue_reason_counts = HashMap::new();
            for task in st.tasks.values() {
                for (reason, count) in &task.reason_counts {
                    *queue_reason_counts.entry(reason.clone()).or_insert(0) += *count;
                }
            }
            let queue = OpsQueueView {
                pending,
                running,
                deduped: st.deduped,
                oldest_wait_seconds,
                reason_counts: queue_reason_counts,
            };

            let workers = st
                .workers
                .iter()
                .map(|w| OpsWorkerView {
                    worker_id: w.worker_id.clone(),
                    state: match w.state {
                        WorkerState::Idle => "idle".to_string(),
                        WorkerState::Running => "running".to_string(),
                        WorkerState::Error => "error".to_string(),
                    },
                    task: w.task.as_ref().map(|k| k.to_view()),
                    started_at: w.started_at.clone(),
                    last_error: w.last_error.as_ref().map(|e| OpsWorkerErrorView {
                        ts: e.ts.clone(),
                        message: e.message.clone(),
                    }),
                })
                .collect::<Vec<_>>();

            let mut tasks = st
                .tasks
                .iter()
                .map(|(k, t)| OpsTaskView {
                    key: k.to_view(),
                    state: match t.state {
                        TaskEntryState::Pending => "pending".to_string(),
                        TaskEntryState::Running { .. } => "running".to_string(),
                    },
                    enqueued_at: t.enqueued_at.clone(),
                    reason_counts: t.reason_counts.clone(),
                    last_run: None,
                })
                .collect::<Vec<_>>();
            tasks.sort_by(|a, b| a.enqueued_at.cmp(&b.enqueued_at));
            tasks.truncate(task_limit as usize);
            (queue, workers, tasks)
        };

        let stats = self.stats(range, now).await?;

        let sparks = self.sparks(range, now).await?;

        let log_tail = self.log_tail(log_limit).await?;

        let mut tasks = tasks;
        if !tasks.is_empty() {
            let last_runs = self.last_runs_for_keys(&tasks).await?;
            for t in tasks.iter_mut() {
                let key = format!("{}:{}", t.key.fid, t.key.gid.clone().unwrap_or_default());
                t.last_run = last_runs.get(&key).cloned();
            }
        }

        let topology = {
            let snap = self.inner.catalog.read().await;
            OpsTopologyView {
                status: snap.topology_status.clone(),
                refreshed_at: snap.topology_refreshed_at.clone(),
                request_count: snap.topology_request_count,
                message: snap.topology_message.clone(),
            }
        };

        Ok(OpsStateSnapshot {
            server_time,
            range: range.as_str().to_string(),
            replay_window_seconds,
            queue,
            workers,
            tasks,
            stats,
            sparks,
            log_tail,
            topology,
        })
    }

    pub async fn stats(
        &self,
        range: OpsRange,
        now: OffsetDateTime,
    ) -> anyhow::Result<OpsStatsView> {
        let cutoff = now
            .saturating_sub(range.duration())
            .format(&Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());

        let row = sqlx::query(
            r#"
SELECT
  COUNT(*) as total,
  SUM(CASE WHEN ok = 1 THEN 1 ELSE 0 END) as success,
  SUM(CASE WHEN fetch_action = 'cache' THEN 1 ELSE 0 END) as cache_hits
FROM ops_task_runs
WHERE ended_at IS NOT NULL
  AND ended_at >= ?
"#,
        )
        .bind(&cutoff)
        .fetch_one(&self.inner.db)
        .await?;
        let total = row.get::<i64, _>(0);
        let success = row.try_get::<i64, _>(1).unwrap_or(0);
        let cache_hits = row.try_get::<i64, _>(2).unwrap_or(0);
        let failure = (total - success).max(0);
        let success_rate_pct = if total > 0 {
            (success as f64) * 100.0 / (total as f64)
        } else {
            0.0
        };

        async fn notify_bucket(
            db: &SqlitePool,
            cutoff: &str,
            channel: &str,
        ) -> anyhow::Result<Option<OpsRateBucketView>> {
            let row = sqlx::query(
                r#"
SELECT
  COUNT(*) as total,
  SUM(CASE WHEN result = 'success' THEN 1 ELSE 0 END) as success,
  SUM(CASE WHEN result = 'error' THEN 1 ELSE 0 END) as failure
FROM ops_notify_runs
WHERE channel = ?
  AND ts >= ?
  AND result IN ('success', 'error')
"#,
            )
            .bind(channel)
            .bind(cutoff)
            .fetch_one(db)
            .await?;
            let total = row.get::<i64, _>(0);
            let success = row.try_get::<i64, _>(1).unwrap_or(0);
            let failure = row.try_get::<i64, _>(2).unwrap_or(0);
            if total == 0 {
                return Ok(None);
            }
            let success_rate_pct = (success as f64) * 100.0 / (total as f64);
            Ok(Some(OpsRateBucketView {
                total,
                success,
                failure,
                success_rate_pct,
                cache_hits: 0,
            }))
        }

        let telegram = notify_bucket(&self.inner.db, &cutoff, "telegram").await?;
        let web_push = notify_bucket(&self.inner.db, &cutoff, "webPush").await?;

        Ok(OpsStatsView {
            collection: OpsRateBucketView {
                total,
                success,
                failure,
                success_rate_pct,
                cache_hits,
            },
            notify: OpsNotifyStatsView { telegram, web_push },
        })
    }

    async fn sparks(&self, range: OpsRange, now: OffsetDateTime) -> anyhow::Result<OpsSparksView> {
        let (bucket_seconds, buckets) = match range {
            OpsRange::H24 => (3600_i64, 24_usize),
            OpsRange::D7 => (86400_i64, 7_usize),
            OpsRange::D30 => (86400_i64, 30_usize),
        };

        let cutoff_dt = now.saturating_sub(range.duration());
        let cutoff = cutoff_dt
            .format(&Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());
        let cutoff_sec = cutoff_dt.unix_timestamp();

        let mut volume = vec![0_i64; buckets];
        let mut collection_total = vec![0_i64; buckets];
        let mut collection_success = vec![0_i64; buckets];

        let rows = sqlx::query(
            r#"
SELECT
  ((CAST(strftime('%s', ended_at) AS INTEGER) - ?) / ?) AS bucket,
  COUNT(*) as total,
  SUM(CASE WHEN ok = 1 THEN 1 ELSE 0 END) as success
FROM ops_task_runs
WHERE ended_at IS NOT NULL
  AND ended_at >= ?
GROUP BY bucket
ORDER BY bucket ASC
"#,
        )
        .bind(cutoff_sec)
        .bind(bucket_seconds)
        .bind(&cutoff)
        .fetch_all(&self.inner.db)
        .await?;

        for r in rows {
            let bucket = r.try_get::<i64, _>(0).unwrap_or(-1);
            if bucket < 0 {
                continue;
            }
            let idx = bucket as usize;
            if idx >= buckets {
                continue;
            }
            let total = r.get::<i64, _>(1);
            let success = r.try_get::<i64, _>(2).unwrap_or(0);
            volume[idx] = total;
            collection_total[idx] = total;
            collection_success[idx] = success;
        }

        async fn notify_counts_by_bucket(
            db: &SqlitePool,
            channel: &str,
            cutoff: &str,
            cutoff_sec: i64,
            bucket_seconds: i64,
            buckets: usize,
        ) -> anyhow::Result<(Vec<i64>, Vec<i64>)> {
            let mut total = vec![0_i64; buckets];
            let mut success = vec![0_i64; buckets];
            let rows = sqlx::query(
                r#"
SELECT
  ((CAST(strftime('%s', ts) AS INTEGER) - ?) / ?) AS bucket,
  COUNT(*) as total,
  SUM(CASE WHEN result = 'success' THEN 1 ELSE 0 END) as success
FROM ops_notify_runs
WHERE channel = ?
  AND ts >= ?
  AND result IN ('success', 'error')
GROUP BY bucket
ORDER BY bucket ASC
"#,
            )
            .bind(cutoff_sec)
            .bind(bucket_seconds)
            .bind(channel)
            .bind(cutoff)
            .fetch_all(db)
            .await?;
            for r in rows {
                let bucket = r.try_get::<i64, _>(0).unwrap_or(-1);
                if bucket < 0 {
                    continue;
                }
                let idx = bucket as usize;
                if idx >= buckets {
                    continue;
                }
                total[idx] = r.get::<i64, _>(1);
                success[idx] = r.try_get::<i64, _>(2).unwrap_or(0);
            }
            Ok((total, success))
        }

        fn pct_series(total: &[i64], success: &[i64]) -> Vec<f64> {
            let mut out = Vec::with_capacity(total.len());
            let mut last = 0.0_f64;
            for i in 0..total.len() {
                let t = total[i].max(0);
                if t == 0 {
                    out.push(last);
                    continue;
                }
                let s = success[i].clamp(0, t);
                last = (s as f64) * 100.0 / (t as f64);
                out.push(last);
            }
            out
        }

        let collection_success_rate_pct = pct_series(&collection_total, &collection_success);
        let (t_total, t_success) = notify_counts_by_bucket(
            &self.inner.db,
            "telegram",
            &cutoff,
            cutoff_sec,
            bucket_seconds,
            buckets,
        )
        .await?;
        let notify_telegram_success_rate_pct = pct_series(&t_total, &t_success);
        let (w_total, w_success) = notify_counts_by_bucket(
            &self.inner.db,
            "webPush",
            &cutoff,
            cutoff_sec,
            bucket_seconds,
            buckets,
        )
        .await?;
        let notify_web_push_success_rate_pct = pct_series(&w_total, &w_success);

        Ok(OpsSparksView {
            bucket_seconds,
            volume,
            collection_success_rate_pct,
            notify_telegram_success_rate_pct,
            notify_web_push_success_rate_pct,
        })
    }

    async fn log_tail(&self, limit: i64) -> anyhow::Result<Vec<OpsLogEntryView>> {
        let rows = sqlx::query(
            r#"
SELECT id, data_json
FROM ops_events
WHERE event = 'ops.log'
ORDER BY id DESC
LIMIT ?
"#,
        )
        .bind(limit)
        .fetch_all(&self.inner.db)
        .await?;

        let mut out = Vec::new();
        for r in rows {
            let event_id = r.get::<i64, _>(0);
            let data_json = r.get::<String, _>(1);
            let parsed: OpsLogPayload = serde_json::from_str(&data_json).unwrap_or(OpsLogPayload {
                ts: "1970-01-01T00:00:00Z".to_string(),
                level: "info".to_string(),
                scope: "ops".to_string(),
                message: data_json,
                meta: None,
            });
            out.push(OpsLogEntryView {
                event_id,
                ts: parsed.ts,
                level: parsed.level,
                scope: parsed.scope,
                message: parsed.message,
                meta: parsed.meta,
            });
        }
        out.reverse();
        Ok(out)
    }

    async fn last_runs_for_keys(
        &self,
        tasks: &[OpsTaskView],
    ) -> anyhow::Result<HashMap<String, OpsTaskLastRunView>> {
        let mut out = HashMap::new();
        for t in tasks {
            let fid = t.key.fid.as_str();
            let gid = t.key.gid.as_deref();
            let row = sqlx::query(
                r#"
SELECT ended_at, ok
FROM ops_task_runs
WHERE fid = ?
  AND ((? IS NULL AND gid IS NULL) OR (? IS NOT NULL AND gid = ?))
  AND ended_at IS NOT NULL
ORDER BY ended_at DESC, id DESC
LIMIT 1
"#,
            )
            .bind(fid)
            .bind(gid)
            .bind(gid)
            .bind(gid)
            .fetch_optional(&self.inner.db)
            .await?;
            if let Some(row) = row {
                let ended_at = row.get::<String, _>(0);
                let ok = row.get::<i64, _>(1) != 0;
                let key = format!("{}:{}", fid, gid.unwrap_or_default());
                out.insert(key, OpsTaskLastRunView { ended_at, ok });
            }
        }
        Ok(out)
    }

    async fn enqueue(
        &self,
        fid: &str,
        gid: Option<&str>,
        reason: &str,
        force_fetch: bool,
        poller_waiter_user_id: Option<&str>,
    ) -> anyhow::Result<oneshot::Receiver<OpsRunOutcome>> {
        let fid = fid.trim();
        if fid.is_empty() {
            anyhow::bail!("fid is empty");
        }
        let reason = reason.trim();
        if reason.is_empty() {
            anyhow::bail!("reason is empty");
        }

        let poller_waiter_user_id = poller_waiter_user_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);

        let key = TaskKey {
            fid: fid.to_string(),
            gid: gid.map(|v| v.trim().to_string()).filter(|v| !v.is_empty()),
        };
        let now = now_rfc3339();

        let (tx, rx) = oneshot::channel();

        let (should_notify, task_event_payload) = {
            let mut st = self.inner.state.lock().await;
            if st.tasks.contains_key(&key) {
                st.deduped += 1;
                let entry = st.tasks.get_mut(&key).unwrap();
                *entry.reason_counts.entry(reason.to_string()).or_insert(0) += 1;
                entry.force_fetch |= force_fetch;
                entry.joiners.push(tx);
                if let Some(user_id) = poller_waiter_user_id.as_ref() {
                    entry.poller_waiter_user_ids.insert(user_id.clone());
                }
                (
                    false,
                    serde_json::json!({
                        "phase": "enqueued",
                        "key": key.to_view(),
                        "reasonCounts": entry.reason_counts.clone(),
                        "run": null,
                    }),
                )
            } else {
                let mut reason_counts = HashMap::new();
                reason_counts.insert(reason.to_string(), 1);
                st.pending.push_back(key.clone());
                st.tasks.insert(
                    key.clone(),
                    TaskEntry {
                        state: TaskEntryState::Pending,
                        enqueued_at: now.clone(),
                        reason_counts,
                        force_fetch,
                        joiners: vec![tx],
                        poller_waiter_user_ids: poller_waiter_user_id.into_iter().collect(),
                    },
                );
                (
                    true,
                    serde_json::json!({
                        "phase": "enqueued",
                        "key": key.to_view(),
                        "reasonCounts": st.tasks.get(&key).map(|t| t.reason_counts.clone()).unwrap_or_default(),
                        "run": null,
                    }),
                )
            }
        };

        let _ = self.publish_event("ops.task", task_event_payload).await;

        if should_notify {
            self.inner.notify.notify_one();
        }

        let _ = self.publish_queue_snapshot().await;

        Ok(rx)
    }

    async fn worker_loop(&self, worker_idx: usize) {
        let upstream = match UpstreamClient::new(self.inner.cfg.upstream_cart_url.clone()) {
            Ok(v) => v,
            Err(err) => {
                let msg = format!("upstream client init failed: {err}");
                let _ = self.set_worker_error(worker_idx, msg).await;
                return;
            }
        };

        loop {
            let key = {
                let mut st = self.inner.state.lock().await;
                st.pending.pop_front()
            };

            let Some(key) = key else {
                self.inner.notify.notified().await;
                continue;
            };

            let started_at = now_rfc3339();
            let run_id = match self.start_task(worker_idx, &key, &started_at).await {
                Ok(id) => id,
                Err(err) => {
                    let _ = self
                        .set_worker_error(worker_idx, format!("start task failed: {err}"))
                        .await;
                    continue;
                }
            };

            let (res, completion) = loop {
                let res = self.run_task(&upstream, &key, run_id).await;
                if matches!(&res, Ok(task_ok) if task_ok.fetch.action == "cache") {
                    if let Some(completion) = self.complete_or_retry_cache_hit(&key).await {
                        break (res, completion);
                    }
                    continue;
                }
                let completion = self.seal_task_for_completion(&key).await;
                break (res, completion);
            };
            let ended_at = now_rfc3339();

            let (ok, fetch, parse, error_code, error_message) = match res {
                Ok(v) => (true, Some(v.fetch), Some(v.parse), None, None),
                Err(err) => (
                    false,
                    err.fetch,
                    err.parse,
                    Some(err.code),
                    Some(err.message),
                ),
            };

            let reason_counts_json = completion.reason_counts_json;

            let _ = sqlx::query(
                r#"
UPDATE ops_task_runs SET
  ended_at = ?,
  ok = ?,
  fetch_action = ?,
  freshness_window_seconds = ?,
  reason_counts_json = ?,
  cache_hit = ?,
  fetch_http_status = ?,
  fetch_bytes = ?,
  fetch_elapsed_ms = ?,
  parse_produced_configs = ?,
  parse_elapsed_ms = ?,
  error_code = ?,
  error_message = ?
WHERE id = ?
"#,
            )
            .bind(&ended_at)
            .bind(if ok { 1 } else { 0 })
            .bind(fetch.as_ref().map(|f| f.action.as_str()).unwrap_or("fetch"))
            .bind(fetch.as_ref().and_then(|f| f.freshness_window_seconds))
            .bind(&reason_counts_json)
            .bind(if fetch.as_ref().is_some_and(|f| f.action == "cache") {
                1
            } else {
                0
            })
            .bind(fetch.as_ref().map(|f| f.http_status as i64))
            .bind(fetch.as_ref().map(|f| f.bytes))
            .bind(fetch.as_ref().map(|f| f.elapsed_ms))
            .bind(parse.as_ref().map(|p| p.produced_configs))
            .bind(parse.as_ref().map(|p| p.elapsed_ms))
            .bind(error_code.as_deref())
            .bind(error_message.as_deref())
            .bind(run_id)
            .execute(&self.inner.db)
            .await;

            let _ = self
                .publish_event(
                    "ops.task",
                    serde_json::json!({
                        "phase": "finished",
                        "key": key.to_view(),
                        "reasonCounts": null,
                        "run": {
                            "runId": run_id,
                            "startedAt": started_at,
                            "endedAt": ended_at,
                            "ok": ok,
                            "fetch": fetch.as_ref().map(|f| serde_json::json!({
                                "url": f.url,
                                "action": f.action,
                                "freshnessWindowSeconds": f.freshness_window_seconds,
                                "httpStatus": f.http_status,
                                "bytes": f.bytes,
                                "elapsedMs": f.elapsed_ms,
                            })),
                            "parse": parse.as_ref().map(|p| serde_json::json!({
                                "ok": p.ok,
                                "producedConfigs": p.produced_configs,
                                "elapsedMs": p.elapsed_ms,
                            })),
                            "error": error_code.as_ref().map(|code| serde_json::json!({
                                "code": code,
                                "message": error_message.clone().unwrap_or_default(),
                            })),
                        }
                    }),
                )
                .await;

            if ok {
                let fetch_action = fetch
                    .as_ref()
                    .map(|item| item.action.as_str())
                    .unwrap_or("fetch");
                let _ = self
                    .log(
                        "info",
                        "ops.task",
                        &format!(
                            "task ok: fid={} gid={} action={fetch_action}",
                            key.fid,
                            key.gid.clone().unwrap_or_default()
                        ),
                        Some(serde_json::json!({
                            "runId": run_id,
                            "fid": key.fid.clone(),
                            "gid": key.gid.clone(),
                            "action": fetch_action,
                            "freshnessWindowSeconds": fetch.as_ref().and_then(|item| item.freshness_window_seconds),
                        })),
                    )
                    .await;
            } else {
                let _ = self
                    .log(
                        "error",
                        "ops.task",
                        &format!(
                            "task failed: fid={} gid={} ({})",
                            key.fid,
                            key.gid.clone().unwrap_or_default(),
                            error_message.clone().unwrap_or_else(|| "unknown".to_string())
                        ),
                        Some(serde_json::json!({ "runId": run_id, "fid": key.fid.clone(), "gid": key.gid.clone() })),
                    )
                    .await;
            }

            let _ = self
                .finish_task(worker_idx, run_id, ok, completion.joiners)
                .await;
            let _ = self.publish_queue_snapshot().await;
        }
    }

    async fn start_task(
        &self,
        worker_idx: usize,
        key: &TaskKey,
        started_at: &str,
    ) -> anyhow::Result<i64> {
        let (run_id, do_lifecycle_notify, reason_counts) = {
            let mut st = self.inner.state.lock().await;
            let w = st
                .workers
                .get_mut(worker_idx)
                .ok_or_else(|| anyhow::anyhow!("worker idx out of bounds"))?;
            w.state = WorkerState::Running;
            w.task = Some(key.clone());
            w.started_at = Some(started_at.to_string());

            let entry = st
                .tasks
                .get_mut(key)
                .ok_or_else(|| anyhow::anyhow!("task missing"))?;
            let do_lifecycle_notify = should_emit_lifecycle_notify(&entry.reason_counts);
            let reason_counts = entry.reason_counts.clone();

            let reason_counts_json =
                serde_json::to_string(&reason_counts).unwrap_or_else(|_| "{}".to_string());
            let res = sqlx::query(
                r#"
INSERT INTO ops_task_runs (
  fid, gid, started_at, ended_at, ok,
  fetch_action, freshness_window_seconds, reason_counts_json, cache_hit
)
VALUES (?, ?, ?, NULL, 0, 'fetch', NULL, ?, 0)
"#,
            )
            .bind(&key.fid)
            .bind(key.gid.as_deref())
            .bind(started_at)
            .bind(&reason_counts_json)
            .execute(&self.inner.db)
            .await?;
            let run_id = res.last_insert_rowid();
            entry.state = TaskEntryState::Running {
                run_id,
                started_at: started_at.to_string(),
            };

            (run_id, do_lifecycle_notify, reason_counts)
        };

        let _ = self.publish_workers_snapshot().await;
        let _ = self.publish_queue_snapshot().await;

        let _ = self
            .publish_event(
                "ops.task",
                serde_json::json!({
                    "phase": "started",
                    "key": key.to_view(),
                    "reasonCounts": reason_counts,
                    "run": {
                        "runId": run_id,
                        "startedAt": started_at,
                        "endedAt": null,
                        "ok": null,
                        "fetch": null,
                        "parse": null,
                        "error": null,
                    }
                }),
            )
            .await;

        if do_lifecycle_notify {
            let _ = self
                .log(
                    "info",
                    "ops.task",
                    "lifecycle notify enabled for this run",
                    Some(serde_json::json!({ "runId": run_id })),
                )
                .await;
        }

        Ok(run_id)
    }

    async fn finish_task(
        &self,
        worker_idx: usize,
        run_id: i64,
        ok: bool,
        joiners: Vec<oneshot::Sender<OpsRunOutcome>>,
    ) -> anyhow::Result<()> {
        {
            let mut st = self.inner.state.lock().await;
            let w = st
                .workers
                .get_mut(worker_idx)
                .ok_or_else(|| anyhow::anyhow!("worker idx out of bounds"))?;
            w.state = WorkerState::Idle;
            w.task = None;
            w.started_at = None;
        }

        for j in joiners {
            let _ = j.send(OpsRunOutcome { run_id, ok });
        }

        let _ = self.publish_workers_snapshot().await;
        Ok(())
    }

    async fn set_worker_error(&self, worker_idx: usize, message: String) -> anyhow::Result<()> {
        let ts = now_rfc3339();
        {
            let mut st = self.inner.state.lock().await;
            let w = st
                .workers
                .get_mut(worker_idx)
                .ok_or_else(|| anyhow::anyhow!("worker idx out of bounds"))?;
            w.state = WorkerState::Error;
            w.last_error = Some(WorkerError {
                ts: ts.clone(),
                message: message.clone(),
            });
        }
        let _ = self.publish_workers_snapshot().await;
        let _ = self
            .log(
                "error",
                "ops.worker",
                &message,
                Some(serde_json::json!({ "workerId": worker_idx + 1 })),
            )
            .await;
        Ok(())
    }

    async fn complete_or_retry_cache_hit(&self, key: &TaskKey) -> Option<TaskCompletion> {
        let mut st = self.inner.state.lock().await;
        if st.tasks.get(key).is_some_and(|entry| entry.force_fetch) {
            return None;
        }
        Some(remove_task_completion(&mut st, key))
    }

    async fn seal_task_for_completion(&self, key: &TaskKey) -> TaskCompletion {
        let mut st = self.inner.state.lock().await;
        remove_task_completion(&mut st, key)
    }

    async fn current_lifecycle_notify_state(
        &self,
        key: &TaskKey,
        fallback_reason_counts: &HashMap<String, i64>,
        fallback_poller_waiter_user_ids: &HashSet<String>,
    ) -> LifecycleNotifyState {
        let st = self.inner.state.lock().await;
        st.tasks
            .get(key)
            .map(|entry| LifecycleNotifyState {
                reason_counts: entry.reason_counts.clone(),
                poller_waiter_user_ids: entry.poller_waiter_user_ids.clone(),
            })
            .unwrap_or_else(|| LifecycleNotifyState {
                reason_counts: fallback_reason_counts.clone(),
                poller_waiter_user_ids: fallback_poller_waiter_user_ids.clone(),
            })
    }

    async fn run_task(
        &self,
        upstream: &UpstreamClient,
        key: &TaskKey,
        run_id: i64,
    ) -> Result<TaskOk, TaskErr> {
        let gid = key.gid.as_deref();
        let url_key = format!("{}:{}", key.fid, gid.unwrap_or("0"));
        let (initial_reason_counts, force_fetch, initial_poller_waiter_user_ids) = {
            let st = self.inner.state.lock().await;
            st.tasks
                .get(key)
                .map(|entry| {
                    (
                        entry.reason_counts.clone(),
                        entry.force_fetch,
                        entry.poller_waiter_user_ids.clone(),
                    )
                })
                .unwrap_or_else(|| (HashMap::new(), false, HashSet::new()))
        };
        let freshness_window_seconds = task_freshness_window_seconds(&initial_reason_counts);

        if !force_fetch {
            if let Some(window) = freshness_window_seconds {
                if let Ok(Some(cache)) =
                    crate::db::get_catalog_url_cache(&self.inner.db, &url_key).await
                {
                    if let Ok(last_success_at) =
                        OffsetDateTime::parse(&cache.last_success_at, &Rfc3339)
                    {
                        let age = OffsetDateTime::now_utc() - last_success_at;
                        if age <= time::Duration::seconds(window) {
                            let produced_configs =
                                serde_json::from_str::<Vec<String>>(&cache.config_ids_json)
                                    .map(|ids| ids.len() as i64)
                                    .unwrap_or(0);
                            #[cfg(test)]
                            pause_before_cache_hit_return().await;
                            return Ok(TaskOk {
                                fetch: TaskFetchMeta {
                                    url: cache.url,
                                    http_status: 0,
                                    bytes: 0,
                                    elapsed_ms: 0,
                                    action: "cache".to_string(),
                                    freshness_window_seconds: Some(window),
                                },
                                parse: TaskParseMeta {
                                    ok: true,
                                    produced_configs,
                                    elapsed_ms: 0,
                                },
                            });
                        }
                    }
                }
            }
        }

        let fetch = match upstream.fetch_region_configs_detailed(&key.fid, gid).await {
            Ok(v) => v,
            Err(err) => {
                return Err(TaskErr {
                    code: "upstream_fetch".to_string(),
                    message: err.to_string(),
                    fetch: None,
                    parse: None,
                })
            }
        };

        let parse = TaskParseMeta {
            ok: true,
            produced_configs: fetch.configs.len() as i64,
            elapsed_ms: fetch.parse_elapsed_ms,
        };
        let region_notice = fetch.region_notice.clone();

        let applied = match crate::db::apply_catalog_url_fetch_success(
            &self.inner.db,
            &key.fid,
            gid,
            &url_key,
            &fetch.url,
            fetch.configs,
            region_notice.as_deref(),
        )
        .await
        {
            Ok(v) => v,
            Err(err) => {
                return Err(TaskErr {
                    code: "apply_failed".to_string(),
                    message: err.to_string(),
                    fetch: Some(TaskFetchMeta {
                        url: fetch.url,
                        http_status: fetch.http_status,
                        bytes: fetch.bytes,
                        elapsed_ms: fetch.elapsed_ms,
                        action: "fetch".to_string(),
                        freshness_window_seconds,
                    }),
                    parse: Some(TaskParseMeta {
                        ok: false,
                        produced_configs: parse.produced_configs,
                        elapsed_ms: parse.elapsed_ms,
                    }),
                })
            }
        };

        {
            let mut snap = self.inner.catalog.write().await;
            snap.fetched_at = applied.fetched_at.clone();
            upsert_region_notice_in_snapshot(&mut snap, &key.fid, gid, region_notice.as_deref());
        }

        let notify_state = self
            .current_lifecycle_notify_state(
                key,
                &initial_reason_counts,
                &initial_poller_waiter_user_ids,
            )
            .await;
        if should_emit_lifecycle_notify(&notify_state.reason_counts)
            && (!applied.listed_event_ids.is_empty()
                || !applied.listed_pending_zero_stock_ids.is_empty()
                || !applied.delisted_ids.is_empty())
        {
            if let Err(err) = self
                .notify_lifecycle_events(
                    run_id,
                    &notify_state.reason_counts,
                    &notify_state.poller_waiter_user_ids,
                    &applied,
                    key,
                )
                .await
            {
                warn!(error = %err, "lifecycle notify failed");
            }
        }

        Ok(TaskOk {
            fetch: TaskFetchMeta {
                url: fetch.url,
                http_status: fetch.http_status,
                bytes: fetch.bytes,
                elapsed_ms: fetch.elapsed_ms,
                action: "fetch".to_string(),
                freshness_window_seconds,
            },
            parse,
        })
    }

    async fn notify_lifecycle_events(
        &self,
        run_id: i64,
        _reason_counts: &HashMap<String, i64>,
        poller_waiter_user_ids: &HashSet<String>,
        applied: &crate::db::ApplyCatalogUrlResult,
        key: &TaskKey,
    ) -> anyhow::Result<()> {
        if applied.listed_event_ids.is_empty()
            && applied.listed_pending_zero_stock_ids.is_empty()
            && applied.delisted_ids.is_empty()
        {
            return Ok(());
        }

        let partition_key = crate::db::monitoring_partition_key(&key.fid, key.gid.as_deref());
        let partition_label =
            load_partition_label(&self.inner.db, &key.fid, key.gid.as_deref()).await?;
        let scope_targets = sqlx::query(
            r#"
SELECT
  s.user_id,
  s.site_base_url,
  s.telegram_enabled,
  s.telegram_bot_token,
  s.telegram_target,
  s.web_push_enabled
FROM settings s
JOIN monitoring_partitions m
  ON m.user_id = s.user_id
 AND m.partition_key = ?
 AND m.enabled = 1
WHERE s.monitoring_events_partition_catalog_change_enabled = 1
"#,
        )
        .bind(&partition_key)
        .fetch_all(&self.inner.db)
        .await?
        .into_iter()
        .map(NotificationDeliveryTarget::from_row)
        .collect::<Vec<_>>();
        let monitored_targets =
            load_monitored_delivery_targets(&self.inner.db, &applied.listed_event_ids).await?;

        let mut ids = applied.listed_event_ids.clone();
        ids.extend(applied.listed_pending_zero_stock_ids.iter().cloned());
        ids.extend(applied.delisted_ids.iter().cloned());
        let config_by_id = load_config_lifecycle_records(&self.inner.db, &ids).await?;

        let listed_records = applied
            .listed_event_ids
            .iter()
            .filter_map(|id| config_by_id.get(id))
            .collect::<Vec<_>>();
        let pending_records = applied
            .listed_pending_zero_stock_ids
            .iter()
            .filter_map(|id| config_by_id.get(id))
            .collect::<Vec<_>>();
        let delisted_records = applied
            .delisted_ids
            .iter()
            .filter_map(|id| config_by_id.get(id))
            .collect::<Vec<_>>();

        for record in &pending_records {
            let msg = format_pending_stock_message(
                &record.name,
                &record.id,
                record.quantity,
                &record.price,
                None,
            );
            let _ = self
                .log(
                    "info",
                    "catalog.listed.pending_stock",
                    &msg,
                    Some(serde_json::json!({
                        "runId": run_id,
                        "fid": key.fid.clone(),
                        "gid": key.gid.clone(),
                        "configId": &record.id,
                    })),
                )
                .await;
        }

        if scope_targets.is_empty() && monitored_targets.is_empty() {
            return Ok(());
        }

        struct MonitoredFallbackContext<'a> {
            listed_records: &'a [&'a ConfigLifecycleRecord],
            key: &'a TaskKey,
            poller_waiter_user_ids: &'a HashSet<String>,
            listed_id_set: &'a HashSet<String>,
        }

        async fn deliver_monitored_restock_fallbacks(
            manager: &OpsManager,
            run_id: i64,
            target: &NotificationDeliveryTarget,
            monitored_ids: &HashSet<String>,
            ctx: &MonitoredFallbackContext<'_>,
        ) -> anyhow::Result<()> {
            for record in ctx.listed_records {
                if !monitored_ids.contains(&record.id) {
                    continue;
                }
                let allow_for_target = if ctx.poller_waiter_user_ids.contains(&target.user_id) {
                    ctx.listed_id_set.contains(&record.id)
                } else {
                    true
                };
                if !allow_for_target {
                    continue;
                }
                let notification = notification_content::build_monitoring_change_notification(
                    &record.name,
                    &notification_content::MonitoringSnapshot {
                        inventory_quantity: 0,
                        price: &record.price,
                        digest: "lifecycle-listed-pending",
                    },
                    &notification_content::MonitoringSnapshot {
                        inventory_quantity: record.quantity,
                        price: &record.price,
                        digest: "lifecycle-listed-pending",
                    },
                    target.site_base_url.as_deref(),
                )
                .expect("synthetic restock notification should exist");
                let msg = format!(
                    "[restock] {} ({}) qty={} price={} {}",
                    record.name,
                    record.id,
                    record.quantity,
                    record.price.amount,
                    target.site_base_url.clone().unwrap_or_default()
                );
                deliver_monitoring_change_notification(
                    manager,
                    run_id,
                    target,
                    &msg,
                    serde_json::json!({
                        "fid": ctx.key.fid.clone(),
                        "gid": ctx.key.gid.clone(),
                        "configId": &record.id,
                        "events": ["restock"],
                        "lifecycleFallback": true,
                    }),
                    &notification,
                )
                .await?;
            }

            Ok(())
        }

        let listed_id_set = applied.listed_ids.iter().cloned().collect::<HashSet<_>>();
        let monitored_fallback_ctx = MonitoredFallbackContext {
            listed_records: &listed_records,
            key,
            poller_waiter_user_ids,
            listed_id_set: &listed_id_set,
        };

        let mut scope_target_user_ids = HashSet::new();

        for target in &scope_targets {
            scope_target_user_ids.insert(target.user_id.clone());

            for record in &pending_records {
                let msg = format_pending_stock_message(
                    &record.name,
                    &record.id,
                    record.quantity,
                    &record.price,
                    target.site_base_url.as_deref(),
                );
                let _ = crate::db::insert_log(
                    &self.inner.db,
                    Some(&target.user_id),
                    "info",
                    "catalog.listed.pending_stock",
                    &msg,
                    Some(serde_json::json!({
                        "fid": key.fid.clone(),
                        "gid": key.gid.clone(),
                        "configId": &record.id,
                        "partitionKey": &partition_key,
                    })),
                )
                .await;
            }

            let monitored_ids = load_enabled_monitoring_ids(
                &self.inner.db,
                &target.user_id,
                &applied.listed_event_ids,
            )
            .await?;

            deliver_monitored_restock_fallbacks(
                self,
                run_id,
                target,
                &monitored_ids,
                &monitored_fallback_ctx,
            )
            .await?;

            for record in &listed_records {
                if monitored_ids.contains(&record.id) {
                    continue;
                }
                let msg = format!(
                    "[config_added] {} ({}) qty={} price={}",
                    record.name, record.id, record.quantity, record.price.amount
                );
                let notification = notification_content::build_config_lifecycle_notification(
                    ConfigLifecycleNotificationKind::Added,
                    &record.name,
                    partition_label.as_deref(),
                    record.quantity,
                    &record.price,
                    target.site_base_url.as_deref(),
                );
                deliver_outbound_notification(
                    self,
                    Some(run_id),
                    target,
                    "catalog.config.added",
                    &msg,
                    serde_json::json!({
                        "fid": &key.fid,
                        "gid": key.gid.as_deref(),
                        "configId": &record.id,
                        "partitionKey": &partition_key,
                        "fetchedAt": &applied.fetched_at,
                        "changeKind": "added",
                    }),
                    &notification,
                )
                .await?;
            }

            for record in &delisted_records {
                let msg = format!(
                    "[config_removed] {} ({}) qty={} price={}",
                    record.name, record.id, record.quantity, record.price.amount
                );
                let notification = notification_content::build_config_lifecycle_notification(
                    ConfigLifecycleNotificationKind::Removed,
                    &record.name,
                    partition_label.as_deref(),
                    record.quantity,
                    &record.price,
                    target.site_base_url.as_deref(),
                );
                deliver_outbound_notification(
                    self,
                    Some(run_id),
                    target,
                    "catalog.config.removed",
                    &msg,
                    serde_json::json!({
                        "fid": &key.fid,
                        "gid": key.gid.as_deref(),
                        "configId": &record.id,
                        "partitionKey": &partition_key,
                        "fetchedAt": &applied.fetched_at,
                        "changeKind": "removed",
                    }),
                    &notification,
                )
                .await?;
            }
        }

        for target in monitored_targets {
            if scope_target_user_ids.contains(&target.user_id) {
                continue;
            }
            let monitored_ids = load_enabled_monitoring_ids(
                &self.inner.db,
                &target.user_id,
                &applied.listed_event_ids,
            )
            .await?;
            if monitored_ids.is_empty() {
                continue;
            }
            deliver_monitored_restock_fallbacks(
                self,
                run_id,
                &target,
                &monitored_ids,
                &monitored_fallback_ctx,
            )
            .await?;
        }

        Ok(())
    }

    pub async fn notify_topology_changes(
        &self,
        added_countries: &[CountryTopologyChange],
        removed_countries: &[CountryTopologyChange],
        added_regions: &[RegionTopologyChange],
        removed_regions: &[RegionTopologyChange],
        added_catalog_fetch_failures: &HashSet<String>,
    ) -> anyhow::Result<()> {
        if added_countries.is_empty()
            && removed_countries.is_empty()
            && added_regions.is_empty()
            && removed_regions.is_empty()
        {
            return Ok(());
        }

        let site_targets = if added_countries.is_empty() && removed_countries.is_empty() {
            Vec::new()
        } else {
            sqlx::query(
                r#"
SELECT
  user_id,
  site_base_url,
  telegram_enabled,
  telegram_bot_token,
  telegram_target,
  web_push_enabled
FROM settings
WHERE monitoring_events_site_region_change_enabled = 1
"#,
            )
            .fetch_all(&self.inner.db)
            .await?
            .into_iter()
            .map(NotificationDeliveryTarget::from_row)
            .collect::<Vec<_>>()
        };

        for change in added_countries {
            if site_targets.is_empty() {
                break;
            }
            let (catalog_items, total_catalog_count) =
                load_country_catalog_summary(&self.inner.db, &change.id).await?;
            let country_key = catalog_region_key(&change.id, None);
            let summary_fetch_failed = added_catalog_fetch_failures.contains(&country_key)
                || added_regions.iter().any(|region| {
                    region.country_id == change.id
                        && added_catalog_fetch_failures.contains(&catalog_region_key(
                            &region.country_id,
                            Some(region.region_id.as_str()),
                        ))
                });
            for target in &site_targets {
                let msg = format!(
                    "[region_added] {} catalogCount={}",
                    change.name, total_catalog_count
                );
                let notification = notification_content::build_topology_notification(
                    TopologyNotificationKind::RegionAdded,
                    &change.name,
                    &catalog_items,
                    total_catalog_count,
                    summary_fetch_failed,
                    target.site_base_url.as_deref(),
                );
                deliver_outbound_notification(
                    self,
                    None,
                    target,
                    "catalog.region.added",
                    &msg,
                    serde_json::json!({
                        "countryId": &change.id,
                        "countryName": &change.name,
                        "catalogCount": total_catalog_count,
                        "changeKind": "added",
                    }),
                    &notification,
                )
                .await?;
            }
        }

        for change in removed_countries {
            for target in &site_targets {
                let msg = format!("[region_removed] {}", change.name);
                let notification = notification_content::build_topology_notification(
                    TopologyNotificationKind::RegionRemoved,
                    &change.name,
                    &[],
                    0,
                    false,
                    target.site_base_url.as_deref(),
                );
                deliver_outbound_notification(
                    self,
                    None,
                    target,
                    "catalog.region.removed",
                    &msg,
                    serde_json::json!({
                        "countryId": &change.id,
                        "countryName": &change.name,
                        "changeKind": "removed",
                    }),
                    &notification,
                )
                .await?;
            }
        }

        let mut region_targets_by_country =
            HashMap::<String, Vec<NotificationDeliveryTarget>>::new();

        for change in added_regions {
            let country_partition_key =
                crate::db::monitoring_partition_key(&change.country_id, None);
            let targets =
                if let Some(existing) = region_targets_by_country.get(&country_partition_key) {
                    existing.clone()
                } else {
                    let loaded = sqlx::query(
                        r#"
SELECT
  s.user_id,
  s.site_base_url,
  s.telegram_enabled,
  s.telegram_bot_token,
  s.telegram_target,
  s.web_push_enabled
FROM settings s
JOIN monitoring_partitions m
  ON m.user_id = s.user_id
 AND m.partition_key = ?
 AND m.enabled = 1
WHERE s.monitoring_events_region_partition_change_enabled = 1
"#,
                    )
                    .bind(&country_partition_key)
                    .fetch_all(&self.inner.db)
                    .await?
                    .into_iter()
                    .map(NotificationDeliveryTarget::from_row)
                    .collect::<Vec<_>>();
                    region_targets_by_country.insert(country_partition_key.clone(), loaded.clone());
                    loaded
                };
            if targets.is_empty() {
                continue;
            }

            let label = format!("{} / {}", change.country_name, change.region_name);
            let (catalog_items, total_catalog_count) =
                load_region_catalog_summary(&self.inner.db, &change.country_id, &change.region_id)
                    .await?;
            let summary_fetch_failed = total_catalog_count == 0
                && added_catalog_fetch_failures.contains(&catalog_region_key(
                    &change.country_id,
                    Some(change.region_id.as_str()),
                ));
            for target in &targets {
                let msg = format!(
                    "[partition_added] {} catalogCount={}",
                    label, total_catalog_count
                );
                let notification = notification_content::build_topology_notification(
                    TopologyNotificationKind::PartitionAdded,
                    &label,
                    &catalog_items,
                    total_catalog_count,
                    summary_fetch_failed,
                    target.site_base_url.as_deref(),
                );
                deliver_outbound_notification(
                    self,
                    None,
                    target,
                    "catalog.partition.added",
                    &msg,
                    serde_json::json!({
                        "countryId": &change.country_id,
                        "countryName": &change.country_name,
                        "regionId": &change.region_id,
                        "regionName": &change.region_name,
                        "catalogCount": total_catalog_count,
                        "changeKind": "added",
                    }),
                    &notification,
                )
                .await?;
            }
        }

        for change in removed_regions {
            let country_partition_key =
                crate::db::monitoring_partition_key(&change.country_id, None);
            let targets =
                if let Some(existing) = region_targets_by_country.get(&country_partition_key) {
                    existing.clone()
                } else {
                    let loaded = sqlx::query(
                        r#"
SELECT
  s.user_id,
  s.site_base_url,
  s.telegram_enabled,
  s.telegram_bot_token,
  s.telegram_target,
  s.web_push_enabled
FROM settings s
JOIN monitoring_partitions m
  ON m.user_id = s.user_id
 AND m.partition_key = ?
 AND m.enabled = 1
WHERE s.monitoring_events_region_partition_change_enabled = 1
"#,
                    )
                    .bind(&country_partition_key)
                    .fetch_all(&self.inner.db)
                    .await?
                    .into_iter()
                    .map(NotificationDeliveryTarget::from_row)
                    .collect::<Vec<_>>();
                    region_targets_by_country.insert(country_partition_key.clone(), loaded.clone());
                    loaded
                };
            if targets.is_empty() {
                continue;
            }

            let label = format!("{} / {}", change.country_name, change.region_name);
            for target in &targets {
                let msg = format!("[partition_removed] {}", label);
                let notification = notification_content::build_topology_notification(
                    TopologyNotificationKind::PartitionRemoved,
                    &label,
                    &[],
                    0,
                    false,
                    target.site_base_url.as_deref(),
                );
                deliver_outbound_notification(
                    self,
                    None,
                    target,
                    "catalog.partition.removed",
                    &msg,
                    serde_json::json!({
                        "countryId": &change.country_id,
                        "countryName": &change.country_name,
                        "regionId": &change.region_id,
                        "regionName": &change.region_name,
                        "changeKind": "removed",
                    }),
                    &notification,
                )
                .await?;
            }
        }

        Ok(())
    }

    async fn publish_queue_snapshot(&self) -> anyhow::Result<i64> {
        let now = OffsetDateTime::now_utc();
        let (pending, running, deduped, oldest_wait_seconds, reason_counts) = {
            let st = self.inner.state.lock().await;
            let pending = st
                .tasks
                .values()
                .filter(|t| matches!(t.state, TaskEntryState::Pending))
                .count() as i64;
            let running = st
                .tasks
                .values()
                .filter(|t| matches!(t.state, TaskEntryState::Running { .. }))
                .count() as i64;
            let oldest_wait_seconds = st
                .tasks
                .values()
                .filter_map(|task| OffsetDateTime::parse(&task.enqueued_at, &Rfc3339).ok())
                .map(|enqueued_at| (now - enqueued_at).whole_seconds().max(0))
                .max();
            let mut reason_counts = HashMap::new();
            for task in st.tasks.values() {
                for (reason, count) in &task.reason_counts {
                    *reason_counts.entry(reason.clone()).or_insert(0) += *count;
                }
            }
            (
                pending,
                running,
                st.deduped,
                oldest_wait_seconds,
                reason_counts,
            )
        };
        self.publish_event(
            "ops.queue",
            serde_json::json!({
                "queue": {
                    "pending": pending,
                    "running": running,
                    "deduped": deduped,
                    "oldestWaitSeconds": oldest_wait_seconds,
                    "reasonCounts": reason_counts,
                },
            }),
        )
        .await
    }

    async fn publish_workers_snapshot(&self) -> anyhow::Result<i64> {
        let workers = {
            let st = self.inner.state.lock().await;
            st.workers
                .iter()
                .map(|w| OpsWorkerView {
                    worker_id: w.worker_id.clone(),
                    state: match w.state {
                        WorkerState::Idle => "idle".to_string(),
                        WorkerState::Running => "running".to_string(),
                        WorkerState::Error => "error".to_string(),
                    },
                    task: w.task.as_ref().map(|k| k.to_view()),
                    started_at: w.started_at.clone(),
                    last_error: w.last_error.as_ref().map(|e| OpsWorkerErrorView {
                        ts: e.ts.clone(),
                        message: e.message.clone(),
                    }),
                })
                .collect::<Vec<_>>()
        };
        self.publish_event("ops.worker", serde_json::json!({ "workers": workers }))
            .await
    }

    async fn publish_event(&self, event: &str, data: serde_json::Value) -> anyhow::Result<i64> {
        self.publish_event_with_ts(event, &now_rfc3339(), data)
            .await
    }

    async fn publish_event_with_ts(
        &self,
        event: &str,
        ts: &str,
        data: serde_json::Value,
    ) -> anyhow::Result<i64> {
        let data_json = serde_json::to_string(&data).unwrap_or_else(|_| "{}".to_string());

        // Serialize publish operations so `broadcast` order matches persisted ids.
        let _guard = self.inner.publish_lock.lock().await;

        let res = sqlx::query("INSERT INTO ops_events (ts, event, data_json) VALUES (?, ?, ?)")
            .bind(ts)
            .bind(event)
            .bind(&data_json)
            .execute(&self.inner.db)
            .await?;
        let id = res.last_insert_rowid();
        let stored = StoredOpsEvent {
            id,
            event: event.to_string(),
            data_json,
            ts: ts.to_string(),
        };
        let _ = self.inner.tx.send(stored.clone());
        Ok(id)
    }
}

#[derive(Debug)]
struct TaskCompletion {
    reason_counts_json: String,
    joiners: Vec<oneshot::Sender<OpsRunOutcome>>,
}

fn remove_task_completion(st: &mut RuntimeState, key: &TaskKey) -> TaskCompletion {
    let task = st.tasks.remove(key);
    let reason_counts_json = task
        .as_ref()
        .map(|entry| {
            serde_json::to_string(&entry.reason_counts).unwrap_or_else(|_| "{}".to_string())
        })
        .unwrap_or_else(|| "{}".to_string());
    let joiners = task.map(|entry| entry.joiners).unwrap_or_default();
    TaskCompletion {
        reason_counts_json,
        joiners,
    }
}

#[derive(Debug)]
struct TaskOk {
    fetch: TaskFetchMeta,
    parse: TaskParseMeta,
}

#[derive(Debug)]
struct TaskErr {
    code: String,
    message: String,
    fetch: Option<TaskFetchMeta>,
    parse: Option<TaskParseMeta>,
}

#[derive(Debug, Clone)]
struct TaskFetchMeta {
    url: String,
    http_status: u16,
    bytes: i64,
    elapsed_ms: i64,
    action: String,
    freshness_window_seconds: Option<i64>,
}

#[derive(Debug, Clone)]
struct TaskParseMeta {
    ok: bool,
    produced_configs: i64,
    elapsed_ms: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpsLogPayload {
    ts: String,
    level: String,
    scope: String,
    message: String,
    meta: Option<serde_json::Value>,
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[cfg(test)]
#[derive(Clone)]
struct CacheHitTestHook {
    entered: Arc<Notify>,
    release: Arc<Notify>,
}

#[cfg(test)]
async fn set_cache_hit_test_hook(hook: Option<CacheHitTestHook>) {
    let store = std::sync::OnceLock::get_or_init(&CACHE_HIT_TEST_HOOK, || Mutex::new(None));
    *store.lock().await = hook;
}

#[cfg(test)]
async fn pause_before_cache_hit_return() {
    let hook = {
        let store = std::sync::OnceLock::get_or_init(&CACHE_HIT_TEST_HOOK, || Mutex::new(None));
        store.lock().await.clone()
    };
    if let Some(hook) = hook {
        hook.entered.notify_waiters();
        hook.release.notified().await;
    }
}

#[cfg(test)]
static CACHE_HIT_TEST_HOOK: std::sync::OnceLock<Mutex<Option<CacheHitTestHook>>> =
    std::sync::OnceLock::new();

fn upsert_region_notice_in_snapshot(
    snap: &mut CatalogSnapshot,
    fid: &str,
    gid: Option<&str>,
    notice: Option<&str>,
) {
    snap.region_notice_initialized_keys
        .insert(crate::upstream::catalog_region_key(fid, gid));
    snap.region_notices
        .retain(|n| !(n.country_id == fid && n.region_id.as_deref() == gid));
    let Some(text) = notice.map(str::trim).filter(|v| !v.is_empty()) else {
        return;
    };
    snap.region_notices.push(crate::models::RegionNotice {
        country_id: fid.to_string(),
        region_id: gid.map(std::string::ToString::to_string),
        text: text.to_string(),
    });
}

// Views / API payload shapes (shared by snapshot + SSE).

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpsTaskKeyView {
    pub fid: String,
    pub gid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpsQueueView {
    pub pending: i64,
    pub running: i64,
    pub deduped: i64,
    pub oldest_wait_seconds: Option<i64>,
    pub reason_counts: HashMap<String, i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpsWorkerErrorView {
    pub ts: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpsWorkerView {
    pub worker_id: String,
    pub state: String,
    pub task: Option<OpsTaskKeyView>,
    pub started_at: Option<String>,
    pub last_error: Option<OpsWorkerErrorView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpsTaskLastRunView {
    pub ended_at: String,
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpsTaskView {
    pub key: OpsTaskKeyView,
    pub state: String,
    pub enqueued_at: String,
    pub reason_counts: HashMap<String, i64>,
    pub last_run: Option<OpsTaskLastRunView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpsRateBucketView {
    pub total: i64,
    pub success: i64,
    pub failure: i64,
    pub success_rate_pct: f64,
    pub cache_hits: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpsNotifyStatsView {
    pub telegram: Option<OpsRateBucketView>,
    pub web_push: Option<OpsRateBucketView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpsStatsView {
    pub collection: OpsRateBucketView,
    pub notify: OpsNotifyStatsView,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpsTopologyView {
    pub status: String,
    pub refreshed_at: Option<String>,
    pub request_count: i64,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpsLogEntryView {
    pub event_id: i64,
    pub ts: String,
    pub level: String,
    pub scope: String,
    pub message: String,
    pub meta: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpsStateSnapshot {
    pub server_time: String,
    pub range: String,
    pub replay_window_seconds: i64,
    pub queue: OpsQueueView,
    pub workers: Vec<OpsWorkerView>,
    pub tasks: Vec<OpsTaskView>,
    pub stats: OpsStatsView,
    pub sparks: OpsSparksView,
    pub log_tail: Vec<OpsLogEntryView>,
    pub topology: OpsTopologyView,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpsSparksView {
    pub bucket_seconds: i64,
    pub volume: Vec<i64>,
    pub collection_success_rate_pct: Vec<f64>,
    pub notify_telegram_success_rate_pct: Vec<f64>,
    pub notify_web_push_success_rate_pct: Vec<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        extract::Query,
        http::StatusCode,
        routing::{get, post},
        Json, Router,
    };
    use serde_json::Value;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use time::format_description::well_known::Rfc3339;

    fn test_config(upstream_cart_url: String) -> RuntimeConfig {
        RuntimeConfig {
            bind_addr: "127.0.0.1:0".to_string(),
            effective_version: "test".to_string(),
            repo_url: "https://example.com/repo".to_string(),
            update_repo: "example/repo".to_string(),
            update_check_enabled: false,
            update_check_ttl_seconds: 0,
            update_check_timeout_ms: 1500,
            github_api_base_url: "https://api.github.com".to_string(),
            upstream_cart_url,
            telegram_api_base_url: "https://api.telegram.org".to_string(),
            auth_user_header: Some("x-user".to_string()),
            dev_user_id: None,
            default_poll_interval_minutes: 1,
            default_poll_jitter_pct: 0.1,
            log_retention_days: 7,
            log_retention_max_rows: 10_000,
            ops_worker_concurrency: 1,
            ops_sse_replay_window_seconds: 3600,
            ops_log_retention_days: 7,
            ops_log_tail_limit_default: 200,
            ops_queue_task_limit_default: 200,
            db_url: "sqlite::memory:".to_string(),
            web_push_vapid_public_key: None,
            web_push_vapid_private_key: None,
            web_push_vapid_subject: None,
            allow_insecure_local_web_push_endpoints: true,
        }
    }

    async fn spawn_stub_server(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{}", addr)
    }

    async fn build_ops_manager_with_config(
        cfg: RuntimeConfig,
        upstream_cart_url: String,
    ) -> (OpsManager, SqlitePool) {
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&cfg.db_url)
            .await
            .unwrap();
        crate::db::init_db(&db).await.unwrap();
        let catalog = Arc::new(RwLock::new(CatalogSnapshot::empty(upstream_cart_url)));
        let ops = OpsManager::new(cfg, db.clone(), catalog);
        ops.start();
        (ops, db)
    }

    async fn build_ops_manager(upstream_cart_url: String) -> (OpsManager, SqlitePool) {
        let cfg = test_config(upstream_cart_url.clone());
        build_ops_manager_with_config(cfg, upstream_cart_url).await
    }

    #[test]
    fn merged_task_prefers_broadest_freshness_window() {
        let reason_counts = HashMap::from([
            ("poller_due".to_string(), 1_i64),
            ("manual_refresh".to_string(), 1_i64),
        ]);
        assert_eq!(
            task_freshness_window_seconds(&reason_counts),
            Some(MANUAL_REFRESH_FRESHNESS_WINDOW_SECONDS)
        );
    }

    #[tokio::test]
    async fn current_lifecycle_notify_state_reads_latest_joiners_and_reasons() {
        let (ops, _db) = build_ops_manager("https://example.invalid/cart".to_string()).await;
        let key = TaskKey {
            fid: "7".to_string(),
            gid: Some("40".to_string()),
        };
        let fallback_reason_counts = HashMap::from([("poller_due".to_string(), 1_i64)]);
        let fallback_waiters = HashSet::new();

        {
            let mut st = ops.inner.state.lock().await;
            st.tasks.insert(
                key.clone(),
                TaskEntry {
                    state: TaskEntryState::Running {
                        run_id: 42,
                        started_at: "2026-03-11T00:00:00Z".to_string(),
                    },
                    enqueued_at: "2026-03-11T00:00:00Z".to_string(),
                    reason_counts: fallback_reason_counts.clone(),
                    force_fetch: false,
                    joiners: Vec::new(),
                    poller_waiter_user_ids: HashSet::new(),
                },
            );
        }

        {
            let mut st = ops.inner.state.lock().await;
            let entry = st.tasks.get_mut(&key).unwrap();
            *entry
                .reason_counts
                .entry("manual_refresh".to_string())
                .or_insert(0) += 1;
            entry
                .poller_waiter_user_ids
                .insert("u_waiting_late".to_string());
        }

        let state = ops
            .current_lifecycle_notify_state(&key, &fallback_reason_counts, &fallback_waiters)
            .await;

        assert_eq!(state.reason_counts.get("poller_due"), Some(&1));
        assert_eq!(state.reason_counts.get("manual_refresh"), Some(&1));
        assert!(state.poller_waiter_user_ids.contains("u_waiting_late"));
    }

    #[tokio::test]
    async fn late_force_fetch_retries_after_cache_hit() {
        #[derive(serde::Deserialize)]
        struct CartQuery {
            fid: Option<String>,
            gid: Option<String>,
        }

        let hits = Arc::new(AtomicUsize::new(0));
        let hits_for_handler = hits.clone();
        let upstream = Router::new().route(
            "/cart",
            get(move |Query(q): Query<CartQuery>| {
                let hits_for_handler = hits_for_handler.clone();
                async move {
                    hits_for_handler.fetch_add(1, Ordering::SeqCst);
                    match (q.fid.as_deref(), q.gid.as_deref()) {
                        (Some("2"), Some("56")) => (
                            StatusCode::OK,
                            include_str!("../tests/fixtures/cart-fid-2-gid-56.html"),
                        ),
                        _ => (StatusCode::NOT_FOUND, "not found"),
                    }
                }
            }),
        );
        let base = spawn_stub_server(upstream).await;
        let (ops, db) = build_ops_manager(format!("{base}/cart")).await;

        let now = OffsetDateTime::now_utc().format(&Rfc3339).unwrap();
        sqlx::query(
            "INSERT INTO catalog_url_cache (url_key, url, config_ids_json, last_success_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("2:56")
        .bind(format!("{base}/cart?fid=2&gid=56"))
        .bind("[]")
        .bind(&now)
        .bind(&now)
        .execute(&db)
        .await
        .unwrap();

        let entered = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        set_cache_hit_test_hook(Some(CacheHitTestHook {
            entered: entered.clone(),
            release: release.clone(),
        }))
        .await;

        ops.enqueue_background("2", Some("56"), "poller_due")
            .await
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), entered.notified())
            .await
            .unwrap();

        let ops_for_force = ops.clone();
        let force_task = tokio::spawn(async move {
            ops_for_force
                .enqueue_and_wait_force_fetch("2", Some("56"), "manual_refresh")
                .await
                .unwrap()
        });
        tokio::task::yield_now().await;
        release.notify_waiters();

        let outcome = tokio::time::timeout(std::time::Duration::from_secs(5), force_task)
            .await
            .unwrap()
            .unwrap();
        set_cache_hit_test_hook(None).await;

        assert!(outcome.ok);
        assert_eq!(hits.load(Ordering::SeqCst), 1);

        let row = sqlx::query(
            "SELECT fetch_action, cache_hit FROM ops_task_runs ORDER BY id DESC LIMIT 1",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>(0), "fetch");
        assert_eq!(row.get::<i64, _>(1), 0);
    }

    async fn seed_catalog_config(db: &SqlitePool, id: &str, name: &str, qty: i64, price: f64) {
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
"#,
        )
        .bind(id)
        .bind("7")
        .bind(Some("40"))
        .bind(name)
        .bind("[]")
        .bind(price)
        .bind("CNY")
        .bind("month")
        .bind(if qty > 0 { "in_stock" } else { "out_of_stock" })
        .bind(qty)
        .bind("2026-03-11T00:00:00Z")
        .bind("digest")
        .bind("2026-03-11T00:00:00Z")
        .bind("2026-03-11T00:00:00Z")
        .bind(if qty > 0 {
            Some("2026-03-11T00:00:00Z")
        } else {
            None
        })
        .bind(Option::<&str>::None)
        .bind(Some("7"))
        .bind(Some("40"))
        .execute(db)
        .await
        .unwrap();
    }

    async fn seed_notification_user(
        db: &SqlitePool,
        cfg: &RuntimeConfig,
        user_id: &str,
        telegram_enabled: bool,
        partition_catalog_change_enabled: bool,
        web_push_enabled: bool,
    ) {
        crate::db::ensure_user(db, cfg, user_id).await.unwrap();
        sqlx::query(
            r#"
UPDATE settings
SET monitoring_events_partition_catalog_change_enabled = ?,
    monitoring_events_region_partition_change_enabled = 0,
    monitoring_events_site_region_change_enabled = 0,
    telegram_enabled = ?,
    telegram_bot_token = ?,
    telegram_target = ?,
    web_push_enabled = ?,
    site_base_url = ?
WHERE user_id = ?
"#,
        )
        .bind(if partition_catalog_change_enabled {
            1
        } else {
            0
        })
        .bind(if telegram_enabled { 1 } else { 0 })
        .bind(if telegram_enabled {
            Some("token")
        } else {
            None
        })
        .bind(if telegram_enabled { Some("chat") } else { None })
        .bind(if web_push_enabled { 1 } else { 0 })
        .bind(Some("https://catnap.example"))
        .bind(user_id)
        .execute(db)
        .await
        .unwrap();

        if partition_catalog_change_enabled {
            crate::db::set_monitoring_partition_enabled(db, user_id, "7", Some("40"), true)
                .await
                .unwrap();
        }
    }

    async fn seed_listed_user(
        db: &SqlitePool,
        cfg: &RuntimeConfig,
        user_id: &str,
        telegram_enabled: bool,
    ) {
        seed_notification_user(db, cfg, user_id, telegram_enabled, true, false).await;
    }

    async fn seed_web_push_subscription(db: &SqlitePool, user_id: &str, endpoint: &str) {
        sqlx::query(
            r#"INSERT INTO web_push_subscriptions (id, user_id, endpoint, p256dh, auth, created_at)
               VALUES (?, ?, ?, ?, ?, ?)"#,
        )
        .bind(format!("sub_{user_id}"))
        .bind(user_id)
        .bind(endpoint)
        .bind("BLMbF9ffKBiWQLCKvTHb6LO8Nb6dcUh6TItC455vu2kElga6PQvUmaFyCdykxY2nOSSL3yKgfbmFLRTUaGv4yV8")
        .bind("xS03Fi5ErfTNH_l9WHE9Ig")
        .bind("2026-01-24T00:00:00Z")
        .execute(db)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn notify_lifecycle_events_logs_pending_stock_without_sending_listed() {
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_for_handler = hits.clone();
        let telegram = Router::new().route(
            "/bottoken/sendMessage",
            post(move || {
                let hits_for_handler = hits_for_handler.clone();
                async move {
                    hits_for_handler.fetch_add(1, Ordering::SeqCst);
                    (StatusCode::OK, r#"{"ok":true}"#)
                }
            }),
        );
        let base = spawn_stub_server(telegram).await;
        let upstream_cart_url = "https://example.invalid/cart".to_string();
        let mut cfg = test_config(upstream_cart_url.clone());
        cfg.telegram_api_base_url = base;
        let (ops, db) = build_ops_manager_with_config(cfg.clone(), upstream_cart_url).await;

        seed_listed_user(&db, &cfg, "u_1", true).await;
        seed_catalog_config(&db, "cfg_pending", "Pending Config", 0, 9.99).await;

        ops.notify_lifecycle_events(
            1,
            &HashMap::from([("manual_refresh".to_string(), 1_i64)]),
            &HashSet::new(),
            &crate::db::ApplyCatalogUrlResult {
                listed_ids: vec!["cfg_pending".to_string()],
                listed_event_ids: Vec::new(),
                listed_pending_zero_stock_ids: vec!["cfg_pending".to_string()],
                delisted_ids: Vec::new(),
                fetched_at: "2026-03-11T00:00:00Z".to_string(),
            },
            &TaskKey {
                fid: "7".to_string(),
                gid: Some("40".to_string()),
            },
        )
        .await
        .unwrap();

        assert_eq!(hits.load(Ordering::SeqCst), 0);

        let pending = sqlx::query(
            "SELECT COUNT(*), MAX(message) FROM event_logs WHERE user_id = ? AND scope = ?",
        )
        .bind("u_1")
        .bind("catalog.listed.pending_stock")
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(pending.get::<i64, _>(0), 1);
        let pending_message = pending.get::<String, _>(1);
        assert!(pending_message.contains("已上架，但当前库存为 0，暂不发送上架通知。"));
        assert!(pending_message.contains("¥9.99 / 月"));
        assert!(!pending_message.contains("[listed-pending-stock]"));

        let listed = sqlx::query("SELECT COUNT(*) FROM event_logs WHERE user_id = ? AND scope = ?")
            .bind("u_1")
            .bind("catalog.config.added")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(listed.get::<i64, _>(0), 0);

        let pending_ops = sqlx::query(
            "SELECT COUNT(*) FROM ops_events WHERE event = 'ops.log' AND json_extract(data_json, '$.scope') = ?",
        )
        .bind("catalog.listed.pending_stock")
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(pending_ops.get::<i64, _>(0), 1);
    }

    #[tokio::test]
    async fn notify_lifecycle_events_logs_pending_stock_without_listed_subscribers() {
        let upstream_cart_url = "https://example.invalid/cart".to_string();
        let (ops, db) = build_ops_manager(upstream_cart_url).await;

        seed_catalog_config(&db, "cfg_pending_only_ops", "Pending Only Ops", 0, 8.88).await;

        ops.notify_lifecycle_events(
            11,
            &HashMap::from([("manual_refresh".to_string(), 1_i64)]),
            &HashSet::new(),
            &crate::db::ApplyCatalogUrlResult {
                listed_ids: vec!["cfg_pending_only_ops".to_string()],
                listed_event_ids: Vec::new(),
                listed_pending_zero_stock_ids: vec!["cfg_pending_only_ops".to_string()],
                delisted_ids: Vec::new(),
                fetched_at: "2026-03-11T00:00:00Z".to_string(),
            },
            &TaskKey {
                fid: "7".to_string(),
                gid: Some("40".to_string()),
            },
        )
        .await
        .unwrap();

        let pending_ops = sqlx::query(
            "SELECT COUNT(*) FROM ops_events WHERE event = 'ops.log' AND json_extract(data_json, '$.scope') = ?",
        )
        .bind("catalog.listed.pending_stock")
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(pending_ops.get::<i64, _>(0), 1);

        let user_logs = sqlx::query("SELECT COUNT(*) FROM event_logs WHERE scope = ?")
            .bind("catalog.listed.pending_stock")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(user_logs.get::<i64, _>(0), 0);
    }

    #[tokio::test]
    async fn config_lifecycle_notifications_only_target_monitored_partition_users() {
        let (ops, db) = build_ops_manager("https://example.com/cart".to_string()).await;

        crate::db::replace_catalog_topology(
            &db,
            "https://example.com/cart",
            &[crate::models::Country {
                id: "7".to_string(),
                name: "德国".to_string(),
            }],
            &[crate::models::Region {
                id: "40".to_string(),
                country_id: "7".to_string(),
                name: "德国特惠".to_string(),
                location_name: None,
            }],
        )
        .await
        .unwrap();
        crate::db::upsert_catalog_configs(
            &db,
            &[crate::upstream::ConfigBase {
                id: "lc:7:40:test".to_string(),
                country_id: "7".to_string(),
                region_id: Some("40".to_string()),
                name: "德国特惠年付 Mini".to_string(),
                specs: vec![],
                price: crate::models::Money {
                    amount: 9.99,
                    currency: "CNY".to_string(),
                    period: "year".to_string(),
                },
                inventory: crate::models::Inventory {
                    status: "in_stock".to_string(),
                    quantity: 2,
                    checked_at: "2026-03-10T00:00:00Z".to_string(),
                },
                digest: "digest-1".to_string(),
                monitor_supported: true,
                source_pid: Some("test".to_string()),
                source_fid: Some("7".to_string()),
                source_gid: Some("40".to_string()),
            }],
        )
        .await
        .unwrap();

        for (user_id, partition_enabled) in [
            ("u_partition_only", true),
            ("u_partition_disabled", false),
            ("u_unmonitored", true),
        ] {
            crate::db::ensure_user(&db, &ops.inner.cfg, user_id)
                .await
                .unwrap();
            sqlx::query(
                r#"
UPDATE settings
SET monitoring_events_partition_catalog_change_enabled = ?,
    monitoring_events_region_partition_change_enabled = 0,
    monitoring_events_site_region_change_enabled = 0,
    telegram_enabled = 0,
    web_push_enabled = 0
WHERE user_id = ?
"#,
            )
            .bind(if partition_enabled { 1 } else { 0 })
            .bind(user_id)
            .execute(&db)
            .await
            .unwrap();
        }

        crate::db::set_monitoring_partition_enabled(&db, "u_partition_only", "7", Some("40"), true)
            .await
            .unwrap();
        crate::db::set_monitoring_partition_enabled(
            &db,
            "u_partition_disabled",
            "7",
            Some("40"),
            true,
        )
        .await
        .unwrap();

        ops.notify_lifecycle_events(
            42,
            &HashMap::from([("manual_refresh".to_string(), 1_i64)]),
            &HashSet::new(),
            &crate::db::ApplyCatalogUrlResult {
                listed_ids: vec!["lc:7:40:test".to_string()],
                listed_event_ids: vec!["lc:7:40:test".to_string()],
                listed_pending_zero_stock_ids: Vec::new(),
                delisted_ids: vec!["lc:7:40:test".to_string()],
                fetched_at: "2026-03-10T00:00:00Z".to_string(),
            },
            &TaskKey {
                fid: "7".to_string(),
                gid: Some("40".to_string()),
            },
        )
        .await
        .unwrap();

        let rows = sqlx::query(
            "SELECT user_id, scope FROM event_logs WHERE scope LIKE 'catalog.config.%' ORDER BY user_id ASC, scope ASC",
        )
        .fetch_all(&db)
        .await
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].get::<String, _>(0), "u_partition_only");
        assert_eq!(rows[0].get::<String, _>(1), "catalog.config.added");
        assert_eq!(rows[1].get::<String, _>(0), "u_partition_only");
        assert_eq!(rows[1].get::<String, _>(1), "catalog.config.removed");
    }

    #[tokio::test]
    async fn config_lifecycle_notifications_route_country_direct_packages_to_country_scope() {
        let (ops, db) = build_ops_manager("https://example.com/cart".to_string()).await;

        crate::db::replace_catalog_topology(
            &db,
            "https://example.com/cart",
            &[crate::models::Country {
                id: "us".to_string(),
                name: "美国".to_string(),
            }],
            &[],
        )
        .await
        .unwrap();
        crate::db::upsert_catalog_configs(
            &db,
            &[crate::upstream::ConfigBase {
                id: "lc:us:default:test".to_string(),
                country_id: "us".to_string(),
                region_id: None,
                name: "VPS • 4C/8G（国家默认）".to_string(),
                specs: vec![],
                price: crate::models::Money {
                    amount: 29.99,
                    currency: "CNY".to_string(),
                    period: "month".to_string(),
                },
                inventory: crate::models::Inventory {
                    status: "in_stock".to_string(),
                    quantity: 7,
                    checked_at: "2026-03-10T00:00:00Z".to_string(),
                },
                digest: "digest-us-default".to_string(),
                monitor_supported: true,
                source_pid: Some("us-default".to_string()),
                source_fid: Some("us".to_string()),
                source_gid: None,
            }],
        )
        .await
        .unwrap();

        crate::db::ensure_user(&db, &ops.inner.cfg, "u_country_scope")
            .await
            .unwrap();
        sqlx::query(
            r#"
UPDATE settings
SET monitoring_events_partition_catalog_change_enabled = 1,
    monitoring_events_region_partition_change_enabled = 0,
    monitoring_events_site_region_change_enabled = 0,
    telegram_enabled = 0,
    web_push_enabled = 0
WHERE user_id = ?
"#,
        )
        .bind("u_country_scope")
        .execute(&db)
        .await
        .unwrap();
        crate::db::set_monitoring_partition_enabled(&db, "u_country_scope", "us", None, true)
            .await
            .unwrap();

        ops.notify_lifecycle_events(
            77,
            &HashMap::from([("manual_refresh".to_string(), 1_i64)]),
            &HashSet::new(),
            &crate::db::ApplyCatalogUrlResult {
                listed_ids: vec!["lc:us:default:test".to_string()],
                listed_event_ids: vec!["lc:us:default:test".to_string()],
                listed_pending_zero_stock_ids: Vec::new(),
                delisted_ids: vec!["lc:us:default:test".to_string()],
                fetched_at: "2026-03-10T00:00:00Z".to_string(),
            },
            &TaskKey {
                fid: "us".to_string(),
                gid: None,
            },
        )
        .await
        .unwrap();

        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM event_logs WHERE user_id = 'u_country_scope' AND scope LIKE 'catalog.config.%'",
        )
        .fetch_one(&db)
        .await
        .unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn topology_notifications_route_to_parent_scopes() {
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_for_handler = hits.clone();
        let telegram = Router::new().route(
            "/bottoken/sendMessage",
            post(move || {
                let hits_for_handler = hits_for_handler.clone();
                async move {
                    hits_for_handler.fetch_add(1, Ordering::SeqCst);
                    (StatusCode::OK, r#"{"ok":true}"#)
                }
            }),
        );
        let base = spawn_stub_server(telegram).await;
        let upstream_cart_url = "https://example.com/cart".to_string();
        let mut cfg = test_config(upstream_cart_url.clone());
        cfg.telegram_api_base_url = base;
        let (ops, db) = build_ops_manager_with_config(cfg.clone(), upstream_cart_url).await;

        crate::db::replace_catalog_topology(
            &db,
            "https://example.com/cart",
            &[crate::models::Country {
                id: "7".to_string(),
                name: "德国".to_string(),
            }],
            &[crate::models::Region {
                id: "40".to_string(),
                country_id: "7".to_string(),
                name: "德国特惠".to_string(),
                location_name: None,
            }],
        )
        .await
        .unwrap();
        crate::db::upsert_catalog_configs(
            &db,
            &[crate::upstream::ConfigBase {
                id: "lc:7:40:test".to_string(),
                country_id: "7".to_string(),
                region_id: Some("40".to_string()),
                name: "德国特惠年付 Mini".to_string(),
                specs: vec![],
                price: crate::models::Money {
                    amount: 9.99,
                    currency: "CNY".to_string(),
                    period: "year".to_string(),
                },
                inventory: crate::models::Inventory {
                    status: "in_stock".to_string(),
                    quantity: 2,
                    checked_at: "2026-03-10T00:00:00Z".to_string(),
                },
                digest: "digest-1".to_string(),
                monitor_supported: true,
                source_pid: Some("test".to_string()),
                source_fid: Some("7".to_string()),
                source_gid: Some("40".to_string()),
            }],
        )
        .await
        .unwrap();

        for (user_id, region_enabled, site_enabled, telegram_enabled) in [
            ("u_region_scope", true, false, true),
            ("u_region_disabled", false, false, false),
            ("u_site_scope", false, true, true),
            ("u_site_disabled", false, false, false),
        ] {
            crate::db::ensure_user(&db, &ops.inner.cfg, user_id)
                .await
                .unwrap();
            sqlx::query(
                r#"
UPDATE settings
SET monitoring_events_partition_catalog_change_enabled = 0,
    monitoring_events_region_partition_change_enabled = ?,
    monitoring_events_site_region_change_enabled = ?,
    telegram_enabled = ?,
    telegram_bot_token = ?,
    telegram_target = ?,
    web_push_enabled = 0
WHERE user_id = ?
"#,
            )
            .bind(if region_enabled { 1 } else { 0 })
            .bind(if site_enabled { 1 } else { 0 })
            .bind(if telegram_enabled { 1 } else { 0 })
            .bind(if telegram_enabled {
                Some("token")
            } else {
                None
            })
            .bind(if telegram_enabled { Some("chat") } else { None })
            .bind(user_id)
            .execute(&db)
            .await
            .unwrap();
        }

        crate::db::set_monitoring_partition_enabled(&db, "u_region_scope", "7", None, true)
            .await
            .unwrap();
        crate::db::set_monitoring_partition_enabled(&db, "u_region_disabled", "7", None, true)
            .await
            .unwrap();

        ops.notify_topology_changes(
            &[CountryTopologyChange {
                id: "8".to_string(),
                name: "芬兰".to_string(),
            }],
            &[CountryTopologyChange {
                id: "9".to_string(),
                name: "冰岛".to_string(),
            }],
            &[RegionTopologyChange {
                country_id: "7".to_string(),
                country_name: "德国".to_string(),
                region_id: "41".to_string(),
                region_name: "德国精品".to_string(),
            }],
            &[RegionTopologyChange {
                country_id: "7".to_string(),
                country_name: "德国".to_string(),
                region_id: "42".to_string(),
                region_name: "德国下架区".to_string(),
            }],
            &HashSet::new(),
        )
        .await
        .unwrap();

        let rows = sqlx::query(
            "SELECT user_id, scope FROM event_logs WHERE scope LIKE 'catalog.%' ORDER BY user_id ASC, scope ASC",
        )
        .fetch_all(&db)
        .await
        .unwrap();
        let actual = rows
            .into_iter()
            .map(|row| (row.get::<String, _>(0), row.get::<String, _>(1)))
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            vec![
                (
                    "u_region_scope".to_string(),
                    "catalog.partition.added".to_string()
                ),
                (
                    "u_region_scope".to_string(),
                    "catalog.partition.removed".to_string()
                ),
                (
                    "u_site_scope".to_string(),
                    "catalog.region.added".to_string()
                ),
                (
                    "u_site_scope".to_string(),
                    "catalog.region.removed".to_string()
                ),
            ]
        );

        assert_eq!(hits.load(Ordering::SeqCst), 4);
        let notify_rows =
            sqlx::query("SELECT COUNT(*) FROM ops_notify_runs WHERE channel = ? AND result = ?")
                .bind("telegram")
                .bind("success")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(notify_rows.get::<i64, _>(0), 4);
    }

    #[tokio::test]
    async fn country_added_notification_marks_partial_summary_failure() {
        let telegram_texts = Arc::new(Mutex::new(Vec::<String>::new()));
        let telegram_texts_for_handler = telegram_texts.clone();
        let telegram = Router::new().route(
            "/bottoken/sendMessage",
            post(move |Json(payload): Json<Value>| {
                let telegram_texts_for_handler = telegram_texts_for_handler.clone();
                async move {
                    if let Some(text) = payload.get("text").and_then(Value::as_str) {
                        telegram_texts_for_handler
                            .lock()
                            .unwrap()
                            .push(text.to_string());
                    }
                    (StatusCode::OK, r#"{"ok":true}"#)
                }
            }),
        );
        let base = spawn_stub_server(telegram).await;
        let upstream_cart_url = "https://example.com/cart".to_string();
        let mut cfg = test_config(upstream_cart_url.clone());
        cfg.telegram_api_base_url = base;
        let (ops, db) = build_ops_manager_with_config(cfg.clone(), upstream_cart_url).await;

        crate::db::replace_catalog_topology(&db, "https://example.com/cart", &[], &[])
            .await
            .unwrap();
        crate::db::upsert_catalog_configs(
            &db,
            &[crate::upstream::ConfigBase {
                id: "lc:8:41:test".to_string(),
                country_id: "8".to_string(),
                region_id: Some("41".to_string()),
                name: "芬兰精品年付 Mini".to_string(),
                specs: vec![],
                price: crate::models::Money {
                    amount: 9.99,
                    currency: "CNY".to_string(),
                    period: "year".to_string(),
                },
                inventory: crate::models::Inventory {
                    status: "in_stock".to_string(),
                    quantity: 2,
                    checked_at: "2026-03-10T00:00:00Z".to_string(),
                },
                digest: "digest-1".to_string(),
                monitor_supported: true,
                source_pid: Some("test".to_string()),
                source_fid: Some("8".to_string()),
                source_gid: Some("41".to_string()),
            }],
        )
        .await
        .unwrap();

        crate::db::ensure_user(&db, &ops.inner.cfg, "u_site_scope")
            .await
            .unwrap();
        sqlx::query(
            r#"
UPDATE settings
SET monitoring_events_site_region_change_enabled = 1,
    telegram_enabled = 1,
    telegram_bot_token = ?,
    telegram_target = ?,
    web_push_enabled = 0
WHERE user_id = ?
"#,
        )
        .bind("token")
        .bind("chat")
        .bind("u_site_scope")
        .execute(&db)
        .await
        .unwrap();

        let mut failures = HashSet::new();
        failures.insert(crate::upstream::catalog_region_key("8", None));

        ops.notify_topology_changes(
            &[CountryTopologyChange {
                id: "8".to_string(),
                name: "芬兰".to_string(),
            }],
            &[],
            &[RegionTopologyChange {
                country_id: "8".to_string(),
                country_name: "芬兰".to_string(),
                region_id: "41".to_string(),
                region_name: "赫尔辛基".to_string(),
            }],
            &[],
            &failures,
        )
        .await
        .unwrap();

        let texts = telegram_texts.lock().unwrap().clone();
        assert_eq!(texts.len(), 1);
        assert!(texts[0].contains("套餐摘要抓取失败，稍后重试。"));
    }

    #[tokio::test]
    async fn notify_lifecycle_events_replaces_listed_with_restock_for_monitored_users() {
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_for_handler = hits.clone();
        let telegram = Router::new().route(
            "/bottoken/sendMessage",
            post(move || {
                let hits_for_handler = hits_for_handler.clone();
                async move {
                    hits_for_handler.fetch_add(1, Ordering::SeqCst);
                    (StatusCode::OK, r#"{"ok":true}"#)
                }
            }),
        );
        let base = spawn_stub_server(telegram).await;
        let upstream_cart_url = "https://example.invalid/cart".to_string();
        let mut cfg = test_config(upstream_cart_url.clone());
        cfg.telegram_api_base_url = base;
        let (ops, db) = build_ops_manager_with_config(cfg.clone(), upstream_cart_url).await;

        seed_listed_user(&db, &cfg, "u_1", true).await;
        seed_catalog_config(&db, "cfg_monitored", "Monitored Config", 2, 19.99).await;
        sqlx::query(
            "INSERT INTO monitoring_configs (user_id, config_id, enabled, created_at, updated_at) VALUES (?, ?, 1, ?, ?)",
        )
        .bind("u_1")
        .bind("cfg_monitored")
        .bind("2026-03-11T00:00:00Z")
        .bind("2026-03-11T00:00:00Z")
        .execute(&db)
        .await
        .unwrap();

        ops.notify_lifecycle_events(
            2,
            &HashMap::from([("poller_due".to_string(), 1_i64)]),
            &HashSet::new(),
            &crate::db::ApplyCatalogUrlResult {
                listed_ids: vec!["cfg_monitored".to_string()],
                listed_event_ids: vec!["cfg_monitored".to_string()],
                listed_pending_zero_stock_ids: Vec::new(),
                delisted_ids: Vec::new(),
                fetched_at: "2026-03-11T00:00:00Z".to_string(),
            },
            &TaskKey {
                fid: "7".to_string(),
                gid: Some("40".to_string()),
            },
        )
        .await
        .unwrap();

        assert_eq!(hits.load(Ordering::SeqCst), 1);

        let listed = sqlx::query("SELECT COUNT(*) FROM event_logs WHERE user_id = ? AND scope = ?")
            .bind("u_1")
            .bind("catalog.config.added")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(listed.get::<i64, _>(0), 0);

        let poll_logs =
            sqlx::query("SELECT COUNT(*) FROM event_logs WHERE user_id = ? AND scope = ?")
                .bind("u_1")
                .bind("poll")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(poll_logs.get::<i64, _>(0), 1);

        let notify_rows = sqlx::query("SELECT COUNT(*) FROM ops_notify_runs")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(notify_rows.get::<i64, _>(0), 1);
    }

    #[tokio::test]
    async fn notify_lifecycle_events_sends_restock_to_non_waiting_monitored_users_on_pure_poller_runs(
    ) {
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_for_handler = hits.clone();
        let telegram = Router::new().route(
            "/bottoken/sendMessage",
            post(move || {
                let hits_for_handler = hits_for_handler.clone();
                async move {
                    hits_for_handler.fetch_add(1, Ordering::SeqCst);
                    (StatusCode::OK, r#"{"ok":true}"#)
                }
            }),
        );
        let base = spawn_stub_server(telegram).await;
        let upstream_cart_url = "https://example.invalid/cart".to_string();
        let mut cfg = test_config(upstream_cart_url.clone());
        cfg.telegram_api_base_url = base;
        let (ops, db) = build_ops_manager_with_config(cfg.clone(), upstream_cart_url).await;

        seed_notification_user(&db, &cfg, "u_cold", true, false, false).await;
        seed_catalog_config(&db, "cfg_pure_poller_cold", "Pure Poller Cold", 3, 18.18).await;
        sqlx::query(
            "INSERT INTO monitoring_configs (user_id, config_id, enabled, created_at, updated_at) VALUES (?, ?, 1, ?, ?)",
        )
        .bind("u_cold")
        .bind("cfg_pure_poller_cold")
        .bind("2026-03-11T00:00:00Z")
        .bind("2026-03-11T00:00:00Z")
        .execute(&db)
        .await
        .unwrap();

        ops.notify_lifecycle_events(
            13,
            &HashMap::from([("poller_due".to_string(), 1_i64)]),
            &HashSet::from(["u_waiting_now".to_string()]),
            &crate::db::ApplyCatalogUrlResult {
                listed_ids: Vec::new(),
                listed_event_ids: vec!["cfg_pure_poller_cold".to_string()],
                listed_pending_zero_stock_ids: Vec::new(),
                delisted_ids: Vec::new(),
                fetched_at: "2026-03-11T00:00:00Z".to_string(),
            },
            &TaskKey {
                fid: "7".to_string(),
                gid: Some("40".to_string()),
            },
        )
        .await
        .unwrap();

        assert_eq!(hits.load(Ordering::SeqCst), 1);

        let poll_logs =
            sqlx::query("SELECT COUNT(*) FROM event_logs WHERE user_id = ? AND scope = ?")
                .bind("u_cold")
                .bind("poll")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(poll_logs.get::<i64, _>(0), 1);
    }

    #[tokio::test]
    async fn notify_lifecycle_events_sends_restock_to_waiting_poller_for_relisted_configs() {
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_for_handler = hits.clone();
        let telegram = Router::new().route(
            "/bottoken/sendMessage",
            post(move || {
                let hits_for_handler = hits_for_handler.clone();
                async move {
                    hits_for_handler.fetch_add(1, Ordering::SeqCst);
                    (StatusCode::OK, r#"{"ok":true}"#)
                }
            }),
        );
        let base = spawn_stub_server(telegram).await;
        let upstream_cart_url = "https://example.invalid/cart".to_string();
        let mut cfg = test_config(upstream_cart_url.clone());
        cfg.telegram_api_base_url = base;
        let (ops, db) = build_ops_manager_with_config(cfg.clone(), upstream_cart_url).await;

        seed_notification_user(&db, &cfg, "u_waiting", true, false, false).await;
        seed_catalog_config(&db, "cfg_relisted", "Relisted Config", 2, 21.21).await;
        sqlx::query(
            "INSERT INTO monitoring_configs (user_id, config_id, enabled, created_at, updated_at) VALUES (?, ?, 1, ?, ?)",
        )
        .bind("u_waiting")
        .bind("cfg_relisted")
        .bind("2026-03-11T00:00:00Z")
        .bind("2026-03-11T00:00:00Z")
        .execute(&db)
        .await
        .unwrap();

        ops.notify_lifecycle_events(
            13,
            &HashMap::from([("poller_due".to_string(), 1_i64)]),
            &HashSet::from(["u_waiting".to_string()]),
            &crate::db::ApplyCatalogUrlResult {
                listed_ids: vec!["cfg_relisted".to_string()],
                listed_event_ids: vec!["cfg_relisted".to_string()],
                listed_pending_zero_stock_ids: Vec::new(),
                delisted_ids: Vec::new(),
                fetched_at: "2026-03-11T00:00:00Z".to_string(),
            },
            &TaskKey {
                fid: "7".to_string(),
                gid: Some("40".to_string()),
            },
        )
        .await
        .unwrap();

        assert_eq!(hits.load(Ordering::SeqCst), 1);

        let poll_logs =
            sqlx::query("SELECT COUNT(*) FROM event_logs WHERE user_id = ? AND scope = ?")
                .bind("u_waiting")
                .bind("poll")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(poll_logs.get::<i64, _>(0), 1);
    }

    #[tokio::test]
    async fn notify_lifecycle_events_skips_fallback_for_waiting_poller_on_mixed_tasks() {
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_for_handler = hits.clone();
        let telegram = Router::new().route(
            "/bottoken/sendMessage",
            post(move || {
                let hits_for_handler = hits_for_handler.clone();
                async move {
                    hits_for_handler.fetch_add(1, Ordering::SeqCst);
                    (StatusCode::OK, r#"{"ok":true}"#)
                }
            }),
        );
        let base = spawn_stub_server(telegram).await;
        let upstream_cart_url = "https://example.invalid/cart".to_string();
        let mut cfg = test_config(upstream_cart_url.clone());
        cfg.telegram_api_base_url = base;
        let (ops, db) = build_ops_manager_with_config(cfg.clone(), upstream_cart_url).await;

        seed_notification_user(&db, &cfg, "u_waiting_mixed", true, false, false).await;
        seed_catalog_config(&db, "cfg_waiting_mixed", "Waiting Mixed", 5, 11.11).await;
        sqlx::query(
            "INSERT INTO monitoring_configs (user_id, config_id, enabled, created_at, updated_at) VALUES (?, ?, 1, ?, ?)",
        )
        .bind("u_waiting_mixed")
        .bind("cfg_waiting_mixed")
        .bind("2026-03-11T00:00:00Z")
        .bind("2026-03-11T00:00:00Z")
        .execute(&db)
        .await
        .unwrap();

        ops.notify_lifecycle_events(
            14,
            &HashMap::from([
                ("poller_due".to_string(), 1_i64),
                ("manual_refresh".to_string(), 1_i64),
            ]),
            &HashSet::from(["u_waiting_mixed".to_string()]),
            &crate::db::ApplyCatalogUrlResult {
                listed_ids: Vec::new(),
                listed_event_ids: vec!["cfg_waiting_mixed".to_string()],
                listed_pending_zero_stock_ids: Vec::new(),
                delisted_ids: Vec::new(),
                fetched_at: "2026-03-11T00:00:00Z".to_string(),
            },
            &TaskKey {
                fid: "7".to_string(),
                gid: Some("40".to_string()),
            },
        )
        .await
        .unwrap();

        assert_eq!(hits.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn notify_lifecycle_events_sends_restock_to_monitored_users_on_non_poller_runs() {
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_for_handler = hits.clone();
        let telegram = Router::new().route(
            "/bottoken/sendMessage",
            post(move || {
                let hits_for_handler = hits_for_handler.clone();
                async move {
                    hits_for_handler.fetch_add(1, Ordering::SeqCst);
                    (StatusCode::OK, r#"{"ok":true}"#)
                }
            }),
        );
        let base = spawn_stub_server(telegram).await;
        let upstream_cart_url = "https://example.invalid/cart".to_string();
        let mut cfg = test_config(upstream_cart_url.clone());
        cfg.telegram_api_base_url = base;
        let (ops, db) = build_ops_manager_with_config(cfg.clone(), upstream_cart_url).await;

        seed_listed_user(&db, &cfg, "u_1", true).await;
        seed_catalog_config(
            &db,
            "cfg_monitored_fallback",
            "Monitored Fallback",
            2,
            18.88,
        )
        .await;
        sqlx::query(
            "INSERT INTO monitoring_configs (user_id, config_id, enabled, created_at, updated_at) VALUES (?, ?, 1, ?, ?)",
        )
        .bind("u_1")
        .bind("cfg_monitored_fallback")
        .bind("2026-03-11T00:00:00Z")
        .bind("2026-03-11T00:00:00Z")
        .execute(&db)
        .await
        .unwrap();

        ops.notify_lifecycle_events(
            12,
            &HashMap::from([("manual_refresh".to_string(), 1_i64)]),
            &HashSet::new(),
            &crate::db::ApplyCatalogUrlResult {
                listed_ids: vec!["cfg_monitored_fallback".to_string()],
                listed_event_ids: vec!["cfg_monitored_fallback".to_string()],
                listed_pending_zero_stock_ids: Vec::new(),
                delisted_ids: Vec::new(),
                fetched_at: "2026-03-11T00:00:00Z".to_string(),
            },
            &TaskKey {
                fid: "7".to_string(),
                gid: Some("40".to_string()),
            },
        )
        .await
        .unwrap();

        assert_eq!(hits.load(Ordering::SeqCst), 1);

        let poll_logs =
            sqlx::query("SELECT COUNT(*) FROM event_logs WHERE user_id = ? AND scope = ?")
                .bind("u_1")
                .bind("poll")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(poll_logs.get::<i64, _>(0), 1);

        let listed_logs =
            sqlx::query("SELECT COUNT(*) FROM event_logs WHERE user_id = ? AND scope = ?")
                .bind("u_1")
                .bind("catalog.config.added")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(listed_logs.get::<i64, _>(0), 0);
    }

    #[tokio::test]
    async fn notify_lifecycle_events_sends_restock_to_monitored_users_without_listed_targets() {
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_for_handler = hits.clone();
        let telegram = Router::new().route(
            "/bottoken/sendMessage",
            post(move || {
                let hits_for_handler = hits_for_handler.clone();
                async move {
                    hits_for_handler.fetch_add(1, Ordering::SeqCst);
                    (StatusCode::OK, r#"{"ok":true}"#)
                }
            }),
        );
        let base = spawn_stub_server(telegram).await;
        let upstream_cart_url = "https://example.invalid/cart".to_string();
        let mut cfg = test_config(upstream_cart_url.clone());
        cfg.telegram_api_base_url = base;
        let (ops, db) = build_ops_manager_with_config(cfg.clone(), upstream_cart_url).await;

        seed_notification_user(&db, &cfg, "u_monitored_only", true, false, false).await;
        seed_catalog_config(&db, "cfg_monitored_only", "Monitored Only", 2, 17.77).await;
        sqlx::query(
            "INSERT INTO monitoring_configs (user_id, config_id, enabled, created_at, updated_at) VALUES (?, ?, 1, ?, ?)",
        )
        .bind("u_monitored_only")
        .bind("cfg_monitored_only")
        .bind("2026-03-11T00:00:00Z")
        .bind("2026-03-11T00:00:00Z")
        .execute(&db)
        .await
        .unwrap();

        ops.notify_lifecycle_events(
            21,
            &HashMap::from([("manual_refresh".to_string(), 1_i64)]),
            &HashSet::new(),
            &crate::db::ApplyCatalogUrlResult {
                listed_ids: vec!["cfg_monitored_only".to_string()],
                listed_event_ids: vec!["cfg_monitored_only".to_string()],
                listed_pending_zero_stock_ids: Vec::new(),
                delisted_ids: Vec::new(),
                fetched_at: "2026-03-11T00:00:00Z".to_string(),
            },
            &TaskKey {
                fid: "7".to_string(),
                gid: Some("40".to_string()),
            },
        )
        .await
        .unwrap();

        assert_eq!(hits.load(Ordering::SeqCst), 1);

        let poll_logs =
            sqlx::query("SELECT COUNT(*) FROM event_logs WHERE user_id = ? AND scope = ?")
                .bind("u_monitored_only")
                .bind("poll")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(poll_logs.get::<i64, _>(0), 1);

        let listed_logs =
            sqlx::query("SELECT COUNT(*) FROM event_logs WHERE user_id = ? AND scope = ?")
                .bind("u_monitored_only")
                .bind("catalog.config.added")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(listed_logs.get::<i64, _>(0), 0);
    }

    #[tokio::test]
    async fn notify_lifecycle_events_keeps_restock_fallback_for_mixed_poller_tasks() {
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_for_handler = hits.clone();
        let telegram = Router::new().route(
            "/bottoken/sendMessage",
            post(move || {
                let hits_for_handler = hits_for_handler.clone();
                async move {
                    hits_for_handler.fetch_add(1, Ordering::SeqCst);
                    (StatusCode::OK, r#"{"ok":true}"#)
                }
            }),
        );
        let base = spawn_stub_server(telegram).await;
        let upstream_cart_url = "https://example.invalid/cart".to_string();
        let mut cfg = test_config(upstream_cart_url.clone());
        cfg.telegram_api_base_url = base;
        let (ops, db) = build_ops_manager_with_config(cfg.clone(), upstream_cart_url).await;

        seed_notification_user(&db, &cfg, "u_mixed", true, false, false).await;
        seed_catalog_config(&db, "cfg_mixed", "Mixed Reasons", 3, 18.01).await;
        sqlx::query(
            "INSERT INTO monitoring_configs (user_id, config_id, enabled, created_at, updated_at) VALUES (?, ?, 1, ?, ?)",
        )
        .bind("u_mixed")
        .bind("cfg_mixed")
        .bind("2026-03-11T00:00:00Z")
        .bind("2026-03-11T00:00:00Z")
        .execute(&db)
        .await
        .unwrap();

        ops.notify_lifecycle_events(
            22,
            &HashMap::from([
                ("poller_due".to_string(), 1_i64),
                ("manual_refresh".to_string(), 1_i64),
            ]),
            &HashSet::new(),
            &crate::db::ApplyCatalogUrlResult {
                listed_ids: vec!["cfg_mixed".to_string()],
                listed_event_ids: vec!["cfg_mixed".to_string()],
                listed_pending_zero_stock_ids: Vec::new(),
                delisted_ids: Vec::new(),
                fetched_at: "2026-03-11T00:00:00Z".to_string(),
            },
            &TaskKey {
                fid: "7".to_string(),
                gid: Some("40".to_string()),
            },
        )
        .await
        .unwrap();

        assert_eq!(hits.load(Ordering::SeqCst), 1);

        let poll_logs =
            sqlx::query("SELECT COUNT(*) FROM event_logs WHERE user_id = ? AND scope = ?")
                .bind("u_mixed")
                .bind("poll")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(poll_logs.get::<i64, _>(0), 1);
    }

    #[tokio::test]
    async fn notify_lifecycle_events_sends_web_push_for_monitored_fallbacks() {
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_for_handler = hits.clone();
        let push = Router::new().route(
            "/*path",
            post(move || {
                let hits_for_handler = hits_for_handler.clone();
                async move {
                    hits_for_handler.fetch_add(1, Ordering::SeqCst);
                    StatusCode::CREATED
                }
            }),
        );
        let push_base = spawn_stub_server(push).await;
        let upstream_cart_url = "https://example.invalid/cart".to_string();
        let mut cfg = test_config(upstream_cart_url.clone());
        cfg.web_push_vapid_private_key =
            Some("IQ9Ur0ykXoHS9gzfYX0aBjy9lvdrjx_PFUXmie9YRcY".to_string());
        cfg.web_push_vapid_subject = Some("mailto:test@example.com".to_string());
        let (ops, db) = build_ops_manager_with_config(cfg.clone(), upstream_cart_url).await;

        seed_notification_user(&db, &cfg, "u_web_push", false, false, true).await;
        seed_web_push_subscription(&db, "u_web_push", &format!("{push_base}/push")).await;
        seed_catalog_config(&db, "cfg_web_push", "Web Push Fallback", 4, 16.66).await;
        sqlx::query(
            "INSERT INTO monitoring_configs (user_id, config_id, enabled, created_at, updated_at) VALUES (?, ?, 1, ?, ?)",
        )
        .bind("u_web_push")
        .bind("cfg_web_push")
        .bind("2026-03-11T00:00:00Z")
        .bind("2026-03-11T00:00:00Z")
        .execute(&db)
        .await
        .unwrap();

        ops.notify_lifecycle_events(
            23,
            &HashMap::from([("manual_refresh".to_string(), 1_i64)]),
            &HashSet::new(),
            &crate::db::ApplyCatalogUrlResult {
                listed_ids: vec!["cfg_web_push".to_string()],
                listed_event_ids: vec!["cfg_web_push".to_string()],
                listed_pending_zero_stock_ids: Vec::new(),
                delisted_ids: Vec::new(),
                fetched_at: "2026-03-11T00:00:00Z".to_string(),
            },
            &TaskKey {
                fid: "7".to_string(),
                gid: Some("40".to_string()),
            },
        )
        .await
        .unwrap();

        assert_eq!(hits.load(Ordering::SeqCst), 1);

        let push_notify_rows =
            sqlx::query("SELECT COUNT(*) FROM ops_notify_runs WHERE channel = ? AND result = ?")
                .bind("webPush")
                .bind("success")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(push_notify_rows.get::<i64, _>(0), 1);
    }

    #[tokio::test]
    async fn notify_lifecycle_events_sends_listed_to_non_monitoring_users() {
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_for_handler = hits.clone();
        let telegram = Router::new().route(
            "/bottoken/sendMessage",
            post(move || {
                let hits_for_handler = hits_for_handler.clone();
                async move {
                    hits_for_handler.fetch_add(1, Ordering::SeqCst);
                    (StatusCode::OK, r#"{"ok":true}"#)
                }
            }),
        );
        let base = spawn_stub_server(telegram).await;
        let upstream_cart_url = "https://example.invalid/cart".to_string();
        let mut cfg = test_config(upstream_cart_url.clone());
        cfg.telegram_api_base_url = base;
        let (ops, db) = build_ops_manager_with_config(cfg.clone(), upstream_cart_url).await;

        seed_listed_user(&db, &cfg, "u_1", true).await;
        seed_catalog_config(&db, "cfg_listed", "Listed Config", 2, 29.99).await;

        ops.notify_lifecycle_events(
            3,
            &HashMap::from([("manual_refresh".to_string(), 1_i64)]),
            &HashSet::new(),
            &crate::db::ApplyCatalogUrlResult {
                listed_ids: vec!["cfg_listed".to_string()],
                listed_event_ids: vec!["cfg_listed".to_string()],
                listed_pending_zero_stock_ids: Vec::new(),
                delisted_ids: Vec::new(),
                fetched_at: "2026-03-11T00:00:00Z".to_string(),
            },
            &TaskKey {
                fid: "7".to_string(),
                gid: Some("40".to_string()),
            },
        )
        .await
        .unwrap();

        assert_eq!(hits.load(Ordering::SeqCst), 1);

        let listed = sqlx::query(
            "SELECT message, meta_json FROM event_logs WHERE user_id = ? AND scope = ?",
        )
        .bind("u_1")
        .bind("catalog.config.added")
        .fetch_one(&db)
        .await
        .unwrap();
        assert!(listed.get::<String, _>(0).starts_with("[config_added]"));
        let meta = serde_json::from_str::<serde_json::Value>(
            &listed.get::<Option<String>, _>(1).expect("meta json"),
        )
        .unwrap();
        assert_eq!(meta["changeKind"].as_str(), Some("added"));

        let notify_rows =
            sqlx::query("SELECT COUNT(*) FROM ops_notify_runs WHERE channel = ? AND result = ?")
                .bind("telegram")
                .bind("success")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(notify_rows.get::<i64, _>(0), 1);
    }
}
