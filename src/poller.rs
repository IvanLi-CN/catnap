use crate::upstream::{catalog_region_key, CatalogSnapshot, UpstreamClient};
use crate::{app::AppState, db};
use crate::{models::Money, notification_content};
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use time::OffsetDateTime;
use tracing::{info, warn};

const INVENTORY_HISTORY_RETENTION_DAYS: i64 = 30;
const DISCOVERY_INTERVAL_SECONDS: i64 = 5 * 60;
const TOPOLOGY_INTERVAL_FALLBACK_MINUTES: i64 = 30;
const TOPOLOGY_RETRY_SECONDS: i64 = 5 * 60;

fn initial_topology_due(
    now: OffsetDateTime,
    topology_interval: time::Duration,
    has_topology: bool,
    last_topology_refresh_at: Option<&str>,
) -> OffsetDateTime {
    if !has_topology {
        return now;
    }

    last_topology_refresh_at
        .and_then(|value| {
            OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339).ok()
        })
        .map(|ts| ts.saturating_add(topology_interval))
        .unwrap_or(now.saturating_add(topology_interval))
}

pub async fn spawn(state: AppState) {
    tokio::spawn(async move {
        if let Err(err) = run(state).await {
            warn!(error = %err, "poller stopped");
        }
    });
}

pub async fn refresh_catalog_topology(state: &AppState, reason: &str) -> anyhow::Result<()> {
    let upstream = UpstreamClient::new(state.config.upstream_cart_url.clone())?;
    let has_existing_catalog_state = db::has_catalog_topology(&state.db).await?
        || !db::list_known_catalog_targets(&state.db).await?.is_empty();
    match upstream.fetch_topology().await {
        Ok(topology) => {
            if topology.countries.is_empty() && has_existing_catalog_state {
                anyhow::bail!("refusing empty topology refresh while catalog state already exists");
            }
            let previous_targets = db::list_catalog_task_keys(&state.db).await?;
            db::replace_catalog_topology(
                &state.db,
                &state.config.upstream_cart_url,
                &topology.countries,
                &topology.regions,
            )
            .await?;
            let current_targets = db::list_catalog_task_keys(&state.db).await?;
            let current_target_keys = current_targets
                .iter()
                .map(|(fid, gid)| catalog_region_key(fid, gid.as_deref()))
                .collect::<HashSet<_>>();
            let removed_targets = previous_targets
                .into_iter()
                .filter(|(fid, gid)| {
                    !current_target_keys.contains(&catalog_region_key(fid, gid.as_deref()))
                })
                .collect::<Vec<_>>();
            let retired_ids = db::retire_catalog_targets(&state.db, &removed_targets).await?;
            let notice_by_key = topology
                .region_notices
                .iter()
                .map(|notice| {
                    (
                        catalog_region_key(&notice.country_id, notice.region_id.as_deref()),
                        notice,
                    )
                })
                .collect::<HashMap<_, _>>();
            for url_key in &topology.region_notice_initialized_keys {
                let (fid, gid) = url_key
                    .split_once(':')
                    .map(|(fid, gid)| {
                        (
                            fid.to_string(),
                            if gid == "0" {
                                None
                            } else {
                                Some(gid.to_string())
                            },
                        )
                    })
                    .unwrap_or_else(|| (url_key.clone(), None));
                let text = notice_by_key
                    .get(url_key)
                    .map(|notice| notice.text.as_str());
                db::set_catalog_region_notice(&state.db, &fid, gid.as_deref(), text).await?;
            }
            apply_topology_snapshot(&state.catalog, topology, &state.config.upstream_cart_url)
                .await;
            let _ = state
                .ops
                .log(
                    "info",
                    "catalog.topology",
                    &format!("topology refresh ok: reason={reason}"),
                    Some(serde_json::json!({
                        "reason": reason,
                        "requestCount": state.catalog.read().await.topology_request_count,
                        "removedTargetCount": removed_targets.len(),
                        "retiredConfigCount": retired_ids.len(),
                    })),
                )
                .await;
            Ok(())
        }
        Err(err) => {
            let message = err.to_string();
            {
                let mut snap = state.catalog.write().await;
                snap.topology_status = "error".to_string();
                snap.topology_message = Some(message.clone());
            }
            let _ = state
                .ops
                .log(
                    "warn",
                    "catalog.topology",
                    "topology refresh failed",
                    Some(serde_json::json!({
                        "reason": reason,
                        "error": message,
                    })),
                )
                .await;
            Err(err)
        }
    }
}

async fn run(state: AppState) -> anyhow::Result<()> {
    let mut next_due: HashMap<String, OffsetDateTime> = HashMap::new();
    let mut last_cleanup: Option<OffsetDateTime> = None;
    let mut next_topology_due: Option<OffsetDateTime> = None;
    let mut next_discovery_due: Option<OffsetDateTime> = None;

    loop {
        let users = sqlx::query("SELECT id FROM users")
            .fetch_all(&state.db)
            .await?
            .into_iter()
            .map(|r| r.get::<String, _>(0))
            .collect::<Vec<_>>();

        let now = OffsetDateTime::now_utc();
        let topology_interval = db::get_global_catalog_refresh_interval_hours(&state.db)
            .await?
            .filter(|hours| *hours > 0)
            .map(time::Duration::hours)
            .unwrap_or_else(|| time::Duration::minutes(TOPOLOGY_INTERVAL_FALLBACK_MINUTES));

        if next_topology_due.is_none() || next_discovery_due.is_none() {
            let has_topology = db::has_catalog_topology(&state.db).await?;
            let has_known_targets = !db::list_known_catalog_targets(&state.db).await?.is_empty();
            let last_topology_refresh_at = if has_topology {
                db::get_catalog_topology_state(&state.db)
                    .await?
                    .map(|row| row.last_topology_refresh_at)
            } else {
                None
            };
            if next_topology_due.is_none() {
                next_topology_due = Some(initial_topology_due(
                    now,
                    topology_interval,
                    has_topology,
                    last_topology_refresh_at.as_deref(),
                ));
            }
            if next_discovery_due.is_none() {
                next_discovery_due = Some(if has_topology || has_known_targets {
                    now.saturating_add(time::Duration::seconds(DISCOVERY_INTERVAL_SECONDS))
                } else {
                    now
                });
            }
        }

        if next_topology_due.is_some_and(|due| now >= due) {
            match refresh_catalog_topology(&state, "scheduler").await {
                Ok(_) => {
                    next_topology_due = Some(now.saturating_add(topology_interval));
                }
                Err(err) => {
                    warn!(error = %err, "topology refresh failed");
                    next_topology_due =
                        Some(now.saturating_add(time::Duration::seconds(TOPOLOGY_RETRY_SECONDS)));
                }
            }
        }

        if next_discovery_due.is_some_and(|due| now >= due) {
            if let Err(err) = enqueue_discovery_targets(&state).await {
                warn!(error = %err, "enqueue discovery targets failed");
            }
            next_discovery_due =
                Some(now.saturating_add(time::Duration::seconds(DISCOVERY_INTERVAL_SECONDS)));
        }

        for user_id in users {
            let settings = db::get_settings(&state.db, &user_id).await?;
            let due = next_due.get(&user_id).copied().unwrap_or(now);
            if now < due {
                continue;
            }

            let interval = settings.poll_interval_minutes.max(1);
            let jitter_pct = settings.poll_jitter_pct.clamp(0.0, 1.0);
            let jitter_s = (interval as f64 * 60.0 * jitter_pct * fastrand::f64()) as i64;
            next_due.insert(
                user_id.clone(),
                now.saturating_add(time::Duration::seconds(interval * 60 + jitter_s)),
            );

            if let Err(err) = poll_once(&state, &user_id, &settings).await {
                warn!(user_id, error = %err, "poll failed");
            }
        }

        if last_cleanup.is_none()
            || last_cleanup.is_some_and(|t| now - t > time::Duration::minutes(30))
        {
            let _ = db::cleanup_logs(
                &state.db,
                state.config.log_retention_days,
                state.config.log_retention_max_rows,
            )
            .await;
            let _ = db::cleanup_ops(&state.db, state.config.ops_log_retention_days).await;
            let _ =
                db::cleanup_inventory_samples_1m(&state.db, INVENTORY_HISTORY_RETENTION_DAYS).await;
            last_cleanup = Some(now);
        }

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

async fn enqueue_discovery_targets(state: &AppState) -> anyhow::Result<()> {
    let targets = known_catalog_targets(state).await?;
    if targets.is_empty() {
        return Ok(());
    }

    for (fid, gid) in targets {
        state
            .ops
            .enqueue_background(&fid, gid.as_deref(), "discovery_due")
            .await?;
    }

    Ok(())
}

pub async fn known_catalog_targets(
    state: &AppState,
) -> anyhow::Result<Vec<(String, Option<String>)>> {
    let mut known = db::list_catalog_task_keys(&state.db).await?;
    known.extend(db::list_known_catalog_targets(&state.db).await?);
    let snapshot = state.catalog.read().await.clone();
    known.extend(catalog_targets_from_snapshot(&snapshot));

    let mut deduped = Vec::new();
    let mut seen = HashSet::new();
    for (fid, gid) in known {
        let key = catalog_region_key(&fid, gid.as_deref());
        if seen.insert(key) {
            deduped.push((fid, gid));
        }
    }
    Ok(deduped)
}

fn catalog_targets_from_snapshot(snapshot: &CatalogSnapshot) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    let mut countries_with_regions = HashSet::new();

    for region in &snapshot.regions {
        countries_with_regions.insert(region.country_id.clone());
        out.push((region.country_id.clone(), Some(region.id.clone())));
    }

    for country in &snapshot.countries {
        if !countries_with_regions.contains(&country.id) {
            out.push((country.id.clone(), None));
        }
    }

    out
}

async fn apply_topology_snapshot(
    catalog: &tokio::sync::RwLock<CatalogSnapshot>,
    topology: crate::upstream::CatalogTopologySnapshot,
    source_url: &str,
) {
    let active_keys = topology
        .countries
        .iter()
        .map(|country| {
            let has_region = topology
                .regions
                .iter()
                .any(|region| region.country_id == country.id);
            if has_region {
                None
            } else {
                Some((country.id.clone(), None))
            }
        })
        .chain(
            topology
                .regions
                .iter()
                .map(|region| Some((region.country_id.clone(), Some(region.id.clone())))),
        )
        .flatten()
        .collect::<HashSet<_>>();
    let active_url_keys = active_keys
        .iter()
        .map(|(fid, gid)| catalog_region_key(fid, gid.as_deref()))
        .collect::<HashSet<_>>();

    let mut snap = catalog.write().await;
    snap.countries = topology.countries;
    snap.regions = topology.regions;
    snap.region_notices.retain(|notice| {
        active_keys.contains(&(notice.country_id.clone(), notice.region_id.clone()))
    });
    for notice in topology.region_notices {
        if let Some(existing) = snap.region_notices.iter_mut().find(|current| {
            current.country_id == notice.country_id && current.region_id == notice.region_id
        }) {
            existing.text = notice.text;
        } else {
            snap.region_notices.push(notice);
        }
    }
    snap.region_notice_initialized_keys
        .retain(|key| active_url_keys.contains(key));
    snap.region_notice_initialized_keys
        .extend(topology.region_notice_initialized_keys);
    snap.source_url = source_url.to_string();
    snap.topology_refreshed_at = Some(topology.refreshed_at);
    snap.topology_request_count = topology.request_count;
    snap.topology_status = "success".to_string();
    snap.topology_message = None;
}

async fn poll_once(
    state: &AppState,
    user_id: &str,
    settings: &db::SettingsRow,
) -> anyhow::Result<()> {
    #[derive(Clone)]
    struct PollState {
        inventory_quantity: i64,
        price: Money,
        digest: String,
    }

    let enabled = db::list_enabled_monitoring_config_ids(&state.db, user_id).await?;
    if enabled.is_empty() {
        return Ok(());
    }

    let mut by_region: HashMap<(String, Option<String>), Vec<String>> = HashMap::new();
    let placeholders = std::iter::repeat_n("?", enabled.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        r#"SELECT id, country_id, region_id FROM catalog_configs WHERE id IN ({placeholders})"#
    );
    let mut q = sqlx::query(&sql);
    for id in &enabled {
        q = q.bind(id);
    }
    let rows = q.fetch_all(&state.db).await?;
    for row in rows {
        let id = row.get::<String, _>(0);
        let fid = row.get::<String, _>(1);
        let gid = row.get::<Option<String>, _>(2);
        by_region.entry((fid, gid)).or_default().push(id);
    }

    for ((fid, gid), ids) in by_region {
        let placeholders = std::iter::repeat_n("?", ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            r#"SELECT id, inventory_quantity, price_amount, price_currency, price_period, config_digest
               FROM catalog_configs
               WHERE id IN ({placeholders})"#
        );
        let mut q = sqlx::query(&sql);
        for id in &ids {
            q = q.bind(id);
        }
        let old_rows = q.fetch_all(&state.db).await?;
        let mut old_by_id: HashMap<String, PollState> = HashMap::new();
        for r in old_rows {
            old_by_id.insert(
                r.get::<String, _>(0),
                PollState {
                    inventory_quantity: r.get::<i64, _>(1),
                    price: Money {
                        amount: r.get::<f64, _>(2),
                        currency: r.get::<String, _>(3),
                        period: r.get::<String, _>(4),
                    },
                    digest: r.get::<String, _>(5),
                },
            );
        }

        let run = state
            .ops
            .enqueue_and_wait(&fid, gid.as_deref(), "poller_due")
            .await?;

        for id in ids {
            let old = old_by_id.get(&id).cloned();
            let new_row = sqlx::query(
                r#"SELECT name, inventory_quantity, price_amount, price_currency, price_period, config_digest
                   FROM catalog_configs
                   WHERE id = ?"#,
            )
            .bind(&id)
            .fetch_optional(&state.db)
            .await?;
            let Some(new_row) = new_row else { continue };
            let new_name = new_row.get::<String, _>(0);
            let new_qty = new_row.get::<i64, _>(1);
            let new_price = new_row.get::<f64, _>(2);
            let new_state = PollState {
                inventory_quantity: new_qty,
                price: Money {
                    amount: new_price,
                    currency: new_row.get::<String, _>(3),
                    period: new_row.get::<String, _>(4),
                },
                digest: new_row.get::<String, _>(5),
            };

            let notification = old.as_ref().and_then(|old_state| {
                notification_content::build_monitoring_change_notification(
                    &new_name,
                    &notification_content::MonitoringSnapshot {
                        inventory_quantity: old_state.inventory_quantity,
                        price: &old_state.price,
                        digest: &old_state.digest,
                    },
                    &notification_content::MonitoringSnapshot {
                        inventory_quantity: new_state.inventory_quantity,
                        price: &new_state.price,
                        digest: &new_state.digest,
                    },
                    settings.site_base_url.as_deref(),
                )
            });

            let events = notification
                .as_ref()
                .map(|item| {
                    item.events
                        .iter()
                        .map(|event| event.as_str())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            if !events.is_empty() {
                let msg = format!(
                    "[{}] {} ({}) qty={} price={} {}",
                    events.join(","),
                    new_name,
                    id,
                    new_qty,
                    new_price,
                    settings.site_base_url.clone().unwrap_or_default()
                );
                db::insert_log(&state.db, Some(user_id), "info", "poll", &msg, None).await?;

                if settings.telegram_enabled {
                    match (
                        settings.telegram_bot_token.as_deref(),
                        settings.telegram_target.as_deref(),
                    ) {
                        (Some(token), Some(target)) => {
                            match crate::notifications::send_telegram(
                                &state.config.telegram_api_base_url,
                                token,
                                target,
                                &notification
                                    .as_ref()
                                    .expect("notification exists when events exist")
                                    .telegram_text,
                            )
                            .await
                            {
                                Ok(_) => {
                                    let _ = state
                                        .ops
                                        .record_notify(run.run_id, "telegram", "success", None)
                                        .await;
                                }
                                Err(err) => {
                                    warn!(user_id, error = %err, "telegram send failed");
                                    let err_msg = err.to_string();
                                    let _ = state
                                        .ops
                                        .record_notify(
                                            run.run_id,
                                            "telegram",
                                            "error",
                                            Some(&err_msg),
                                        )
                                        .await;
                                    db::insert_log(
                                        &state.db,
                                        Some(user_id),
                                        "warn",
                                        "notify.telegram",
                                        "telegram send failed",
                                        Some(serde_json::json!({ "error": err.to_string() })),
                                    )
                                    .await?;
                                }
                            }
                        }
                        _ => {
                            let _ = state
                                .ops
                                .record_notify(
                                    run.run_id,
                                    "telegram",
                                    "skipped",
                                    Some("missing telegram config"),
                                )
                                .await;
                        }
                    }
                }

                let _ = state
                    .ops
                    .log(
                        "info",
                        "poll.result",
                        &msg,
                        Some(serde_json::json!({
                            "runId": run.run_id,
                            "userId": user_id,
                            "configId": id,
                            "events": events,
                        })),
                    )
                    .await;
            }
        }

        info!(
            user_id,
            fid,
            gid = gid.as_deref().unwrap_or(""),
            "poll updated"
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_topology_due_is_immediate_when_topology_is_missing() {
        let now = OffsetDateTime::UNIX_EPOCH;
        let due = initial_topology_due(now, time::Duration::minutes(30), false, None);
        assert_eq!(due, now);
    }

    #[test]
    fn initial_topology_due_uses_last_refresh_when_topology_exists() {
        let now = OffsetDateTime::UNIX_EPOCH;
        let last = "1970-01-01T00:05:00Z";
        let due = initial_topology_due(now, time::Duration::minutes(30), true, Some(last));
        assert_eq!(
            due,
            OffsetDateTime::UNIX_EPOCH + time::Duration::minutes(35)
        );
    }
}
