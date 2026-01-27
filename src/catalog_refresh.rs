use crate::app::AppState;
use crate::db;
use crate::models::{CatalogRefreshCurrent, CatalogRefreshStatus};
use crate::upstream::UpstreamClient;
use std::collections::HashMap;
use std::sync::Arc;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::sync::{broadcast, Mutex};
use tracing::{info, warn};
use uuid::Uuid;

const MANUAL_MIN_INTERVAL_SECONDS: i64 = 30;
const FULL_REFRESH_CACHE_HIT_SECONDS: i64 = 5 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshTrigger {
    Manual,
    Auto,
}

impl RefreshTrigger {
    fn as_str(self) -> &'static str {
        match self {
            RefreshTrigger::Manual => "manual",
            RefreshTrigger::Auto => "auto",
        }
    }
}

#[derive(Debug)]
pub enum TriggerError {
    RateLimited,
    Internal(anyhow::Error),
}

#[derive(Clone)]
pub struct CatalogRefreshManager {
    inner: Arc<Inner>,
}

struct Inner {
    status: Mutex<CatalogRefreshStatus>,
    tx: broadcast::Sender<CatalogRefreshStatus>,
    manual_gate: Mutex<HashMap<String, OffsetDateTime>>,
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

impl CatalogRefreshManager {
    pub fn new() -> Self {
        let initial = CatalogRefreshStatus {
            job_id: Uuid::new_v4().to_string(),
            state: "idle".to_string(),
            trigger: RefreshTrigger::Manual.as_str().to_string(),
            done: 0,
            total: 0,
            message: None,
            started_at: now_rfc3339(),
            updated_at: now_rfc3339(),
            current: None,
        };
        let (tx, _) = broadcast::channel(128);
        Self {
            inner: Arc::new(Inner {
                status: Mutex::new(initial),
                tx,
                manual_gate: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<CatalogRefreshStatus> {
        self.inner.tx.subscribe()
    }

    pub async fn status(&self) -> CatalogRefreshStatus {
        self.inner.status.lock().await.clone()
    }

    pub async fn trigger(
        &self,
        app: AppState,
        trigger: RefreshTrigger,
        user_id_for_rate_limit: Option<&str>,
    ) -> Result<CatalogRefreshStatus, TriggerError> {
        if trigger == RefreshTrigger::Manual {
            if let Some(user_id) = user_id_for_rate_limit {
                let now = OffsetDateTime::now_utc();
                let mut gate = self.inner.manual_gate.lock().await;
                if let Some(last) = gate.get(user_id).copied() {
                    if now - last < time::Duration::seconds(MANUAL_MIN_INTERVAL_SECONDS) {
                        return Err(TriggerError::RateLimited);
                    }
                }
                gate.insert(user_id.to_string(), now);
            }
        }

        let (job_id, st_snapshot) = {
            let mut st = self.inner.status.lock().await;
            if st.state == "running" {
                return Ok(st.clone());
            }

            let job_id = Uuid::new_v4().to_string();
            let started_at = now_rfc3339();
            st.job_id = job_id.clone();
            st.state = "running".to_string();
            st.trigger = trigger.as_str().to_string();
            st.done = 0;
            st.total = 0;
            st.message = None;
            st.started_at = started_at.clone();
            st.updated_at = started_at.clone();
            st.current = None;
            let snapshot = st.clone();
            let _ = self.inner.tx.send(snapshot.clone());
            (job_id, snapshot)
        };

        let this = self.clone();
        tokio::spawn(async move {
            if let Err(err) = run_full_refresh_job(app, this.clone(), job_id, trigger).await {
                warn!(error = %err, "catalog refresh job failed");
                this.finish_error(err.to_string()).await;
            }
        });

        Ok(st_snapshot)
    }

    async fn finish_error(&self, message: String) {
        let now = now_rfc3339();
        let mut st = self.inner.status.lock().await;
        st.state = "error".to_string();
        st.message = Some(message);
        st.updated_at = now;
        st.current = None;
        let _ = self.inner.tx.send(st.clone());
    }

    async fn finish_success(&self) {
        let now = now_rfc3339();
        let mut st = self.inner.status.lock().await;
        st.state = "success".to_string();
        st.message = None;
        st.updated_at = now;
        st.current = None;
        let _ = self.inner.tx.send(st.clone());
    }

    async fn update_progress(&self, done: i64, total: i64, current: Option<CatalogRefreshCurrent>) {
        let now = now_rfc3339();
        let mut st = self.inner.status.lock().await;
        st.done = done;
        st.total = total;
        st.updated_at = now;
        st.current = current;
        let _ = self.inner.tx.send(st.clone());
    }
}

impl Default for CatalogRefreshManager {
    fn default() -> Self {
        Self::new()
    }
}

async fn run_full_refresh_job(
    app: AppState,
    mgr: CatalogRefreshManager,
    job_id: String,
    trigger: RefreshTrigger,
) -> anyhow::Result<()> {
    let upstream = UpstreamClient::new(app.config.upstream_cart_url.clone())?;

    // Enumerate URL tasks by parsing the upstream cart root.
    let root_html = upstream
        .fetch_html_raw(&app.config.upstream_cart_url)
        .await?;
    let countries = crate::upstream::parse_countries(&root_html);

    let mut regions = Vec::new();
    let mut tasks: Vec<(String, Option<String>)> = Vec::new();

    for c in &countries {
        let fid = &c.id;
        let fid_url = format!("{}?fid={fid}", app.config.upstream_cart_url);
        let fid_html = upstream.fetch_html_raw(&fid_url).await?;

        let mut fid_regions = crate::upstream::parse_regions(fid, &fid_html);
        if fid_regions.is_empty() {
            tasks.push((fid.clone(), None));
        } else {
            regions.append(&mut fid_regions);
            for r in regions.iter().filter(|r| &r.country_id == fid) {
                tasks.push((fid.clone(), Some(r.id.clone())));
            }
        }
    }

    {
        let mut snap = app.catalog.write().await;
        snap.countries = countries;
        snap.regions = regions;
        snap.source_url = app.config.upstream_cart_url.clone();
    }

    let total = tasks.len().max(1) as i64;
    mgr.update_progress(0, total, None).await;
    info!(
        job_id,
        trigger = trigger.as_str(),
        total,
        "catalog refresh started"
    );

    let mut done: i64 = 0;
    for (fid, gid) in tasks {
        let gid_part = gid.as_deref().unwrap_or("0");
        let url_key = format!("{fid}:{gid_part}");
        let url = if let Some(gid) = gid.as_deref() {
            format!("{}?fid={fid}&gid={gid}", app.config.upstream_cart_url)
        } else {
            format!("{}?fid={fid}", app.config.upstream_cart_url)
        };

        let mut action = "fetch".to_string();
        let mut note: Option<String> = None;
        if let Some(cache) = db::get_catalog_url_cache(&app.db, &url_key).await? {
            if let Ok(last) = OffsetDateTime::parse(&cache.last_success_at, &Rfc3339) {
                let now = OffsetDateTime::now_utc();
                if now - last <= time::Duration::seconds(FULL_REFRESH_CACHE_HIT_SECONDS) {
                    action = "cache".to_string();
                    note = Some("cache hit".to_string());
                }
            }
        }

        mgr.update_progress(
            done,
            total,
            Some(CatalogRefreshCurrent {
                url_key: url_key.clone(),
                url: url.clone(),
                action: action.clone(),
                note: note.clone(),
            }),
        )
        .await;

        if action == "fetch" {
            let _ = app
                .ops
                .enqueue_and_wait(&fid, gid.as_deref(), "manual_refresh")
                .await?;
        }

        done += 1;
        mgr.update_progress(done, total, None).await;
    }

    mgr.finish_success().await;
    info!(job_id, done, total, "catalog refresh finished");
    Ok(())
}
