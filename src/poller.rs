use crate::{app::AppState, db, upstream::UpstreamClient};
use sqlx::Row;
use std::collections::HashMap;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tracing::{info, warn};

const INVENTORY_HISTORY_RETENTION_DAYS: i64 = 30;

pub async fn spawn(state: AppState) {
    tokio::spawn(async move {
        if let Err(err) = run(state).await {
            warn!(error = %err, "poller stopped");
        }
    });
}

async fn run(state: AppState) -> anyhow::Result<()> {
    let upstream = UpstreamClient::new(state.config.upstream_cart_url.clone())?;
    let mut next_due: HashMap<String, OffsetDateTime> = HashMap::new();
    let mut last_cleanup: Option<OffsetDateTime> = None;

    loop {
        let users = sqlx::query("SELECT id FROM users")
            .fetch_all(&state.db)
            .await?
            .into_iter()
            .map(|r| r.get::<String, _>(0))
            .collect::<Vec<_>>();

        let now = OffsetDateTime::now_utc();
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

            if let Err(err) = poll_once(&state, &upstream, &user_id, &settings).await {
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
            let _ =
                db::cleanup_inventory_samples_1m(&state.db, INVENTORY_HISTORY_RETENTION_DAYS).await;
            last_cleanup = Some(now);
        }

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

async fn poll_once(
    state: &AppState,
    upstream: &UpstreamClient,
    user_id: &str,
    settings: &db::SettingsRow,
) -> anyhow::Result<()> {
    let enabled = db::list_enabled_monitoring_config_ids(&state.db, user_id).await?;
    if enabled.is_empty() {
        return Ok(());
    }

    let snapshot = state.catalog.read().await.clone();
    let mut by_region: HashMap<(String, Option<String>), Vec<String>> = HashMap::new();
    for id in enabled.iter() {
        if let Some(c) = snapshot.configs.iter().find(|c| &c.id == id) {
            by_region
                .entry((c.country_id.clone(), c.region_id.clone()))
                .or_default()
                .push(c.id.clone());
        }
    }

    for ((fid, gid), ids) in by_region {
        let parsed = upstream.fetch_region_configs(&fid, gid.as_deref()).await?;

        let mut parsed_map: HashMap<String, crate::upstream::ConfigBase> = HashMap::new();
        for c in parsed {
            parsed_map.insert(c.id.clone(), c);
        }

        for id in ids {
            let Some(new_cfg) = parsed_map.get(&id) else {
                continue;
            };
            let old_row = sqlx::query(
                "SELECT inventory_quantity, price_amount, config_digest FROM catalog_configs WHERE id = ?",
            )
            .bind(&id)
            .fetch_optional(&state.db)
            .await?;

            let mut events = Vec::new();
            if let Some(old) = old_row {
                let old_qty = old.get::<i64, _>(0);
                let old_price = old.get::<f64, _>(1);
                let old_digest = old.get::<String, _>(2);

                if old_qty == 0 && new_cfg.inventory.quantity > 0 {
                    events.push("restock");
                }
                if (old_price - new_cfg.price.amount).abs() > f64::EPSILON {
                    events.push("price");
                }
                if old_digest != new_cfg.digest {
                    events.push("config");
                }
            }

            if !events.is_empty() {
                let msg = format!(
                    "[{}] {} ({}) qty={} price={} {}",
                    events.join(","),
                    new_cfg.name,
                    id,
                    new_cfg.inventory.quantity,
                    new_cfg.price.amount,
                    settings.site_base_url.clone().unwrap_or_default()
                );
                db::insert_log(&state.db, Some(user_id), "info", "poll", &msg, None).await?;

                if settings.telegram_enabled {
                    if let (Some(token), Some(target)) = (
                        settings.telegram_bot_token.as_deref(),
                        settings.telegram_target.as_deref(),
                    ) {
                        if let Err(err) = crate::notifications::send_telegram(
                            &state.config.telegram_api_base_url,
                            token,
                            target,
                            &msg,
                        )
                        .await
                        {
                            warn!(user_id, error = %err, "telegram send failed");
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
            }
        }

        // Update snapshot + DB for all parsed configs (sampling covers all configs we refreshed).
        let changed = parsed_map.values().cloned().collect::<Vec<_>>();
        if !changed.is_empty() {
            db::upsert_catalog_configs(&state.db, &changed).await?;
            let mut lock = state.catalog.write().await;
            for c in lock.configs.iter_mut() {
                if let Some(new_cfg) = parsed_map.get(&c.id) {
                    *c = new_cfg.clone();
                }
            }
            lock.fetched_at = OffsetDateTime::now_utc()
                .format(&Rfc3339)
                .unwrap_or_else(|_| lock.fetched_at.clone());
            info!(
                user_id,
                fid,
                gid = gid.as_deref().unwrap_or(""),
                "poll updated"
            );
        }
    }

    Ok(())
}
