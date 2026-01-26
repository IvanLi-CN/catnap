use crate::config::RuntimeConfig;
use crate::notifications;
use crate::upstream::{CatalogSnapshot, UpstreamClient};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::sync::{broadcast, oneshot, Mutex, Notify, RwLock};
use tracing::warn;

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
    joiners: Vec<oneshot::Sender<OpsRunOutcome>>,
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
        let rx = self.enqueue(fid, gid, reason).await?;
        rx.await.map_err(|_| anyhow::anyhow!("ops task canceled"))
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
            let queue = OpsQueueView {
                pending,
                running,
                deduped: st.deduped,
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
  SUM(CASE WHEN ok = 1 THEN 1 ELSE 0 END) as success
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
    ) -> anyhow::Result<oneshot::Receiver<OpsRunOutcome>> {
        let fid = fid.trim();
        if fid.is_empty() {
            anyhow::bail!("fid is empty");
        }
        let reason = reason.trim();
        if reason.is_empty() {
            anyhow::bail!("reason is empty");
        }

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
                entry.joiners.push(tx);
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
                        joiners: vec![tx],
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

            let res = self.run_task(&upstream, &key, run_id).await;
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

            let _ = sqlx::query(
                r#"
UPDATE ops_task_runs SET
  ended_at = ?,
  ok = ?,
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
                let _ = self
                    .log(
                        "info",
                        "ops.task",
                        &format!(
                            "task ok: fid={} gid={}",
                            key.fid,
                            key.gid.clone().unwrap_or_default()
                        ),
                        Some(serde_json::json!({ "runId": run_id, "fid": key.fid.clone(), "gid": key.gid.clone() })),
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

            let _ = self.finish_task(worker_idx, &key, run_id, ok).await;
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
            let do_lifecycle_notify = entry.reason_counts.contains_key("manual_refresh");
            let reason_counts = entry.reason_counts.clone();

            let res = sqlx::query(
                r#"
INSERT INTO ops_task_runs (fid, gid, started_at, ended_at, ok)
VALUES (?, ?, ?, NULL, 0)
"#,
            )
            .bind(&key.fid)
            .bind(key.gid.as_deref())
            .bind(started_at)
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
        key: &TaskKey,
        run_id: i64,
        ok: bool,
    ) -> anyhow::Result<()> {
        let joiners = {
            let mut st = self.inner.state.lock().await;
            let w = st
                .workers
                .get_mut(worker_idx)
                .ok_or_else(|| anyhow::anyhow!("worker idx out of bounds"))?;
            w.state = WorkerState::Idle;
            w.task = None;
            w.started_at = None;

            st.tasks.remove(key).map(|t| t.joiners).unwrap_or_default()
        };

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

    async fn run_task(
        &self,
        upstream: &UpstreamClient,
        key: &TaskKey,
        run_id: i64,
    ) -> Result<TaskOk, TaskErr> {
        let gid = key.gid.as_deref();
        let url_key = format!("{}:{}", key.fid, gid.unwrap_or("0"));

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

        let applied = match crate::db::apply_catalog_url_fetch_success(
            &self.inner.db,
            &key.fid,
            gid,
            &url_key,
            &fetch.url,
            fetch.configs,
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
        }

        let do_lifecycle_notify = {
            let st = self.inner.state.lock().await;
            st.tasks
                .get(key)
                .map(|t| t.reason_counts.contains_key("manual_refresh"))
                .unwrap_or(false)
        };

        if do_lifecycle_notify
            && (!applied.listed_ids.is_empty() || !applied.delisted_ids.is_empty())
        {
            if let Err(err) = self.notify_lifecycle_events(run_id, &applied, key).await {
                warn!(error = %err, "lifecycle notify failed");
            }
        }

        Ok(TaskOk {
            fetch: TaskFetchMeta {
                url: fetch.url,
                http_status: fetch.http_status,
                bytes: fetch.bytes,
                elapsed_ms: fetch.elapsed_ms,
            },
            parse,
        })
    }

    async fn notify_lifecycle_events(
        &self,
        run_id: i64,
        applied: &crate::db::ApplyCatalogUrlResult,
        key: &TaskKey,
    ) -> anyhow::Result<()> {
        let mut targets_listed = Vec::new();
        let mut targets_delisted = Vec::new();

        if !applied.listed_ids.is_empty() {
            targets_listed = sqlx::query(
                r#"
SELECT
  user_id,
  site_base_url,
  telegram_enabled,
  telegram_bot_token,
  telegram_target,
  web_push_enabled
FROM settings
WHERE monitoring_events_listed_enabled = 1
"#,
            )
            .fetch_all(&self.inner.db)
            .await?;
        }
        if !applied.delisted_ids.is_empty() {
            targets_delisted = sqlx::query(
                r#"
SELECT
  user_id,
  site_base_url,
  telegram_enabled,
  telegram_bot_token,
  telegram_target,
  web_push_enabled
FROM settings
WHERE monitoring_events_delisted_enabled = 1
"#,
            )
            .fetch_all(&self.inner.db)
            .await?;
        }

        if targets_listed.is_empty() && targets_delisted.is_empty() {
            return Ok(());
        }

        async fn load_configs(
            db: &SqlitePool,
            ids: &[String],
        ) -> anyhow::Result<Vec<(String, String, f64, i64)>> {
            if ids.is_empty() {
                return Ok(Vec::new());
            }
            let placeholders = std::iter::repeat_n("?", ids.len())
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                r#"
SELECT id, name, price_amount, inventory_quantity
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
                .map(|r| {
                    (
                        r.get::<String, _>(0),
                        r.get::<String, _>(1),
                        r.get::<f64, _>(2),
                        r.get::<i64, _>(3),
                    )
                })
                .collect())
        }

        let listed = load_configs(&self.inner.db, &applied.listed_ids).await?;
        let delisted = load_configs(&self.inner.db, &applied.delisted_ids).await?;

        for row in targets_listed {
            let user_id = row.get::<String, _>(0);
            let site_base_url = row.get::<Option<String>, _>(1);
            let tg_enabled = row.get::<i64, _>(2) != 0;
            let tg_bot_token = row.get::<Option<String>, _>(3);
            let tg_target = row.get::<Option<String>, _>(4);
            let wp_enabled = row.get::<i64, _>(5) != 0;

            for (id, name, price, qty) in listed.iter() {
                let url = site_base_url.as_deref().unwrap_or("").trim_end_matches('/');
                let msg =
                    format!("[listed] {name} ({id}) qty={qty} price={price} {url}/monitoring");
                let _ = crate::db::insert_log(
                    &self.inner.db,
                    Some(&user_id),
                    "info",
                    "catalog.listed",
                    &msg,
                    Some(serde_json::json!({ "fid": key.fid.clone(), "gid": key.gid.clone() })),
                )
                .await;
                let _ = self
                    .log(
                        "info",
                        "catalog.listed",
                        &msg,
                        Some(serde_json::json!({ "runId": run_id, "userId": user_id })),
                    )
                    .await;

                if tg_enabled {
                    match (tg_bot_token.as_deref(), tg_target.as_deref()) {
                        (Some(token), Some(target)) => match notifications::send_telegram(
                            &self.inner.cfg.telegram_api_base_url,
                            token,
                            target,
                            &msg,
                        )
                        .await
                        {
                            Ok(_) => {
                                let _ = self
                                    .record_notify(run_id, "telegram", "success", None)
                                    .await;
                            }
                            Err(err) => {
                                let err_msg = err.to_string();
                                let _ = self
                                    .record_notify(run_id, "telegram", "error", Some(&err_msg))
                                    .await;
                                let _ = crate::db::insert_log(
                                    &self.inner.db,
                                    Some(&user_id),
                                    "warn",
                                    "notify.telegram",
                                    "telegram send failed",
                                    Some(serde_json::json!({ "error": err.to_string() })),
                                )
                                .await;
                            }
                        },
                        _ => {
                            let _ = self
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

                if wp_enabled {
                    match crate::db::get_latest_web_push_subscription(&self.inner.db, &user_id)
                        .await
                    {
                        Ok(Some(sub)) => match notifications::send_web_push(
                            &self.inner.cfg,
                            &sub,
                            "catnap",
                            &msg,
                            "/monitoring",
                        )
                        .await
                        {
                            Ok(_) => {
                                let _ =
                                    self.record_notify(run_id, "webPush", "success", None).await;
                            }
                            Err(err) => {
                                let err_msg = err.to_string();
                                let _ = self
                                    .record_notify(run_id, "webPush", "error", Some(&err_msg))
                                    .await;
                            }
                        },
                        Ok(None) => {
                            let _ = self
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
                            let _ = self
                                .record_notify(run_id, "webPush", "error", Some(&err_msg))
                                .await;
                        }
                    }
                }
            }
        }

        for row in targets_delisted {
            let user_id = row.get::<String, _>(0);
            let site_base_url = row.get::<Option<String>, _>(1);
            let tg_enabled = row.get::<i64, _>(2) != 0;
            let tg_bot_token = row.get::<Option<String>, _>(3);
            let tg_target = row.get::<Option<String>, _>(4);
            let wp_enabled = row.get::<i64, _>(5) != 0;

            for (id, name, price, qty) in delisted.iter() {
                let url = site_base_url.as_deref().unwrap_or("").trim_end_matches('/');
                let msg =
                    format!("[delisted] {name} ({id}) qty={qty} price={price} {url}/monitoring");
                let _ = crate::db::insert_log(
                    &self.inner.db,
                    Some(&user_id),
                    "info",
                    "catalog.delisted",
                    &msg,
                    Some(serde_json::json!({ "fid": key.fid.clone(), "gid": key.gid.clone() })),
                )
                .await;
                let _ = self
                    .log(
                        "info",
                        "catalog.delisted",
                        &msg,
                        Some(serde_json::json!({ "runId": run_id, "userId": user_id })),
                    )
                    .await;

                if tg_enabled {
                    match (tg_bot_token.as_deref(), tg_target.as_deref()) {
                        (Some(token), Some(target)) => match notifications::send_telegram(
                            &self.inner.cfg.telegram_api_base_url,
                            token,
                            target,
                            &msg,
                        )
                        .await
                        {
                            Ok(_) => {
                                let _ = self
                                    .record_notify(run_id, "telegram", "success", None)
                                    .await;
                            }
                            Err(err) => {
                                let err_msg = err.to_string();
                                let _ = self
                                    .record_notify(run_id, "telegram", "error", Some(&err_msg))
                                    .await;
                                let _ = crate::db::insert_log(
                                    &self.inner.db,
                                    Some(&user_id),
                                    "warn",
                                    "notify.telegram",
                                    "telegram send failed",
                                    Some(serde_json::json!({ "error": err.to_string() })),
                                )
                                .await;
                            }
                        },
                        _ => {
                            let _ = self
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

                if wp_enabled {
                    match crate::db::get_latest_web_push_subscription(&self.inner.db, &user_id)
                        .await
                    {
                        Ok(Some(sub)) => match notifications::send_web_push(
                            &self.inner.cfg,
                            &sub,
                            "catnap",
                            &msg,
                            "/monitoring",
                        )
                        .await
                        {
                            Ok(_) => {
                                let _ =
                                    self.record_notify(run_id, "webPush", "success", None).await;
                            }
                            Err(err) => {
                                let err_msg = err.to_string();
                                let _ = self
                                    .record_notify(run_id, "webPush", "error", Some(&err_msg))
                                    .await;
                            }
                        },
                        Ok(None) => {
                            let _ = self
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
                            let _ = self
                                .record_notify(run_id, "webPush", "error", Some(&err_msg))
                                .await;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn publish_queue_snapshot(&self) -> anyhow::Result<i64> {
        let (pending, running, deduped) = {
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
            (pending, running, st.deduped)
        };
        self.publish_event(
            "ops.queue",
            serde_json::json!({
                "queue": { "pending": pending, "running": running, "deduped": deduped },
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
