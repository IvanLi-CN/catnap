use crate::{app::AppState, db, upstream::UpstreamClient};
use sqlx::Row;
use std::collections::HashMap;
use time::OffsetDateTime;
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
    let mut auto_last_trigger: Option<OffsetDateTime> = None;
    let mut auto_interval_hours: Option<i64> = None;

    loop {
        let users = sqlx::query("SELECT id FROM users")
            .fetch_all(&state.db)
            .await?
            .into_iter()
            .map(|r| r.get::<String, _>(0))
            .collect::<Vec<_>>();

        let now = OffsetDateTime::now_utc();

        // Global auto refresh scheduler: use the minimum enabled interval across all users.
        let next_interval_hours = db::get_global_catalog_refresh_interval_hours(&state.db).await?;
        if next_interval_hours != auto_interval_hours {
            auto_interval_hours = next_interval_hours;
        }
        match auto_interval_hours {
            Some(hours) if hours > 0 => {
                // Start counting from "now" when auto refresh becomes enabled.
                if auto_last_trigger.is_none() {
                    auto_last_trigger = Some(now);
                }
                let last = auto_last_trigger.unwrap_or(now);
                let due = last.saturating_add(time::Duration::hours(hours));
                if now >= due {
                    let _ = state
                        .catalog_refresh
                        .trigger(
                            state.clone(),
                            crate::catalog_refresh::RefreshTrigger::Auto,
                            None,
                        )
                        .await;
                    auto_last_trigger = Some(now);
                }
            }
            _ => {
                auto_last_trigger = None;
            }
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

    let mut by_region: HashMap<(String, Option<String>), Vec<String>> = HashMap::new();
    let placeholders = std::iter::repeat_n("?", enabled.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        r#"SELECT id, country_id, region_id FROM catalog_configs WHERE id IN ({placeholders})"#
    );
    let mut q = sqlx::query(&sql);
    for id in enabled.iter() {
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
            r#"SELECT id, inventory_quantity, price_amount, config_digest
               FROM catalog_configs
               WHERE id IN ({placeholders})"#
        );
        let mut q = sqlx::query(&sql);
        for id in ids.iter() {
            q = q.bind(id);
        }
        let old_rows = q.fetch_all(&state.db).await?;
        let mut old_by_id: HashMap<String, (i64, f64, String)> = HashMap::new();
        for r in old_rows {
            old_by_id.insert(
                r.get::<String, _>(0),
                (
                    r.get::<i64, _>(1),
                    r.get::<f64, _>(2),
                    r.get::<String, _>(3),
                ),
            );
        }

        // Fetch + apply shared URL task (dedup across monitoring + full refresh).
        let _ = state
            .catalog_refresh
            .fetch_and_apply_region(state, upstream, &fid, gid.as_deref())
            .await?;

        for id in ids {
            let old = old_by_id.get(&id).cloned();
            let new_row = sqlx::query(
                r#"SELECT name, inventory_quantity, price_amount, config_digest
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
            let new_digest = new_row.get::<String, _>(3);

            let mut events = Vec::new();
            if let Some((old_qty, old_price, old_digest)) = old {
                if old_qty == 0 && new_qty > 0 {
                    events.push("restock");
                }
                if (old_price - new_price).abs() > f64::EPSILON {
                    events.push("price");
                }
                if old_digest != new_digest {
                    events.push("config");
                }
            }

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

        info!(
            user_id,
            fid,
            gid = gid.as_deref().unwrap_or(""),
            "poll updated"
        );
    }

    Ok(())
}
