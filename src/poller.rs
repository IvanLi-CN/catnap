use crate::{app::AppState, db};
use crate::{models::Money, notification_content};
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
            r#"SELECT id, inventory_quantity, price_amount, price_currency, price_period, config_digest
               FROM catalog_configs
               WHERE id IN ({placeholders})"#
        );
        let mut q = sqlx::query(&sql);
        for id in ids.iter() {
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

        // Fetch + apply via global ops queue (dedup across poller + refresh).
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
