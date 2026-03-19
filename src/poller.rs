use crate::defaults::{
    FIXED_CATALOG_TOPOLOGY_PROBE_INTERVAL_MINUTES, FIXED_CATALOG_TOPOLOGY_REFRESH_INTERVAL_HOURS,
};
use crate::upstream::{
    catalog_region_key, parse_configs, parse_region_notice, parse_regions,
    retain_country_direct_configs, CatalogSnapshot, UpstreamClient,
};
use crate::{app::AppState, db};
use crate::{models::Money, notification_content};
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use time::OffsetDateTime;
use tracing::{info, warn};

const INVENTORY_HISTORY_RETENTION_DAYS: i64 = 30;
const DISCOVERY_INTERVAL_SECONDS: i64 = 5 * 60;
const TOPOLOGY_RETRY_SECONDS: i64 = 5 * 60;

#[derive(Debug, Default, PartialEq, Eq)]
struct TopologyNotificationChanges {
    added_countries: Vec<crate::ops::CountryTopologyChange>,
    removed_countries: Vec<crate::ops::CountryTopologyChange>,
    added_regions: Vec<crate::ops::RegionTopologyChange>,
    removed_regions: Vec<crate::ops::RegionTopologyChange>,
}

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
        Ok(mut topology) => {
            if topology.countries.is_empty() && has_existing_catalog_state {
                anyhow::bail!("refusing empty topology refresh while catalog state already exists");
            }
            let previous_snapshot = state.catalog.read().await.clone();
            let preserved_ambiguous_countries =
                preserve_ambiguous_country_regions(&mut topology, &previous_snapshot);
            let addition_topology =
                merge_topology_probe_result(&previous_snapshot, topology.clone());
            let added_changes = collect_topology_notification_changes(
                &previous_snapshot,
                &addition_topology.countries,
                &addition_topology.regions,
            );
            let removed_changes = collect_topology_notification_changes(
                &previous_snapshot,
                &topology.countries,
                &topology.regions,
            );
            let previous_targets = known_catalog_targets(state).await?;
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
            persist_topology_region_notices(state, &topology).await?;
            let added_catalog_fetch_failures = prefetch_added_target_catalogs(
                state,
                &upstream,
                &added_changes.added_countries,
                &added_changes.added_regions,
            )
            .await;
            if let Err(err) = state
                .ops
                .notify_topology_changes(
                    &added_changes.added_countries,
                    &removed_changes.removed_countries,
                    &added_changes.added_regions,
                    &removed_changes.removed_regions,
                    &added_catalog_fetch_failures,
                )
                .await
            {
                warn!(error = %err, "topology lifecycle notify failed");
            }
            let request_count = topology.request_count;
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
                        "requestCount": request_count,
                        "removedTargetCount": removed_targets.len(),
                        "retiredConfigCount": retired_ids.len(),
                        "preservedAmbiguousCountryCount": preserved_ambiguous_countries.len(),
                    })),
                )
                .await;
            Ok(())
        }
        Err(err) => {
            log_topology_failure(state, reason, "topology refresh failed", &err).await;
            Err(err)
        }
    }
}

pub async fn probe_catalog_topology(state: &AppState, reason: &str) -> anyhow::Result<()> {
    let upstream = UpstreamClient::new(state.config.upstream_cart_url.clone())?;
    let has_existing_catalog_state = db::has_catalog_topology(&state.db).await?
        || !db::list_known_catalog_targets(&state.db).await?.is_empty();
    match upstream.fetch_topology().await {
        Ok(mut topology) => {
            if topology.countries.is_empty() && has_existing_catalog_state {
                anyhow::bail!("refusing empty topology refresh while catalog state already exists");
            }
            let previous_snapshot = state.catalog.read().await.clone();
            let preserved_ambiguous_countries =
                preserve_ambiguous_country_regions(&mut topology, &previous_snapshot);
            let previous_target_keys = known_catalog_targets(state)
                .await?
                .into_iter()
                .map(|(fid, gid)| catalog_region_key(&fid, gid.as_deref()))
                .collect::<HashSet<_>>();
            let topology = merge_topology_probe_result(&previous_snapshot, topology);
            let added_changes = collect_topology_notification_changes(
                &previous_snapshot,
                &topology.countries,
                &topology.regions,
            );
            db::replace_catalog_topology(
                &state.db,
                &state.config.upstream_cart_url,
                &topology.countries,
                &topology.regions,
            )
            .await?;
            persist_topology_region_notices(state, &topology).await?;
            let added_catalog_fetch_failures = prefetch_added_target_catalogs(
                state,
                &upstream,
                &added_changes.added_countries,
                &added_changes.added_regions,
            )
            .await;
            if let Err(err) = state
                .ops
                .notify_topology_changes(
                    &added_changes.added_countries,
                    &[],
                    &added_changes.added_regions,
                    &[],
                    &added_catalog_fetch_failures,
                )
                .await
            {
                warn!(error = %err, "topology lifecycle notify failed");
            }
            let discovered_target_count =
                topology
                    .countries
                    .iter()
                    .filter(|country| {
                        !topology
                            .regions
                            .iter()
                            .any(|region| region.country_id == country.id)
                    })
                    .map(|country| catalog_region_key(&country.id, None))
                    .chain(topology.regions.iter().map(|region| {
                        catalog_region_key(&region.country_id, Some(region.id.as_str()))
                    }))
                    .filter(|key| !previous_target_keys.contains(key))
                    .count();
            let request_count = topology.request_count;
            apply_topology_snapshot(&state.catalog, topology, &state.config.upstream_cart_url)
                .await;
            let _ = state
                .ops
                .log(
                    "info",
                    "catalog.topology",
                    &format!("topology probe ok: reason={reason}"),
                    Some(serde_json::json!({
                        "reason": reason,
                        "requestCount": request_count,
                        "discoveredTargetCount": discovered_target_count,
                        "preservedAmbiguousCountryCount": preserved_ambiguous_countries.len(),
                    })),
                )
                .await;
            Ok(())
        }
        Err(err) => {
            log_topology_failure(state, reason, "topology probe failed", &err).await;
            Err(err)
        }
    }
}

async fn log_topology_failure(state: &AppState, reason: &str, message: &str, err: &anyhow::Error) {
    let error_message = err.to_string();
    {
        let mut snap = state.catalog.write().await;
        snap.topology_status = "error".to_string();
        snap.topology_message = Some(error_message.clone());
    }
    let _ = state
        .ops
        .log(
            "warn",
            "catalog.topology",
            message,
            Some(serde_json::json!({
                "reason": reason,
                "error": error_message,
            })),
        )
        .await;
}

async fn persist_topology_region_notices(
    state: &AppState,
    topology: &crate::upstream::CatalogTopologySnapshot,
) -> anyhow::Result<()> {
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
    Ok(())
}

fn build_added_topology_targets(
    added_countries: &[crate::ops::CountryTopologyChange],
    added_regions: &[crate::ops::RegionTopologyChange],
) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    for country in added_countries {
        let key = catalog_region_key(&country.id, None);
        if seen.insert(key) {
            out.push((country.id.clone(), None));
        }
    }

    for region in added_regions {
        let key = catalog_region_key(&region.country_id, Some(region.region_id.as_str()));
        if seen.insert(key) {
            out.push((region.country_id.clone(), Some(region.region_id.clone())));
        }
    }

    out
}

async fn prefetch_added_target_catalogs(
    state: &AppState,
    upstream: &UpstreamClient,
    added_countries: &[crate::ops::CountryTopologyChange],
    added_regions: &[crate::ops::RegionTopologyChange],
) -> HashSet<String> {
    let mut failed_target_keys = HashSet::new();
    let added_country_ids = added_countries
        .iter()
        .map(|country| country.id.clone())
        .collect::<HashSet<_>>();

    for country in added_countries {
        let fid = country.id.clone();
        let root_url = format!("{}?fid={fid}", state.config.upstream_cart_url);
        let root_key = catalog_region_key(&fid, None);

        let root_html = match upstream.fetch_html_raw(&root_url).await {
            Ok(html) => Some(html),
            Err(err) => {
                warn!(error = %err, fid, gid = ?Option::<String>::None, "prefetch added target catalog failed");
                failed_target_keys.insert(root_key.clone());
                None
            }
        };

        let mut region_configs = Vec::new();
        for region in added_regions
            .iter()
            .filter(|region| region.country_id == fid)
        {
            let gid = region.region_id.clone();
            let region_url = format!("{}?fid={fid}&gid={gid}", state.config.upstream_cart_url);
            let region_key = catalog_region_key(&fid, Some(gid.as_str()));

            match upstream
                .fetch_region_configs_detailed(&fid, Some(gid.as_str()))
                .await
            {
                Ok(fetch) => {
                    let apply_result = db::apply_catalog_url_fetch_success(
                        &state.db,
                        &fid,
                        Some(gid.as_str()),
                        &region_key,
                        &region_url,
                        fetch.configs.clone(),
                        db::CatalogUrlFetchHints {
                            region_notice: fetch.region_notice.as_deref(),
                            empty_result_authoritative: fetch.empty_result_authoritative,
                        },
                    )
                    .await;
                    match apply_result {
                        Ok(_) => region_configs.extend(fetch.configs),
                        Err(err) => {
                            warn!(error = %err, fid, gid = ?Some(gid.clone()), "prefetch added target catalog failed");
                            failed_target_keys.insert(region_key);
                        }
                    }
                }
                Err(err) => {
                    warn!(error = %err, fid, gid = ?Some(gid.clone()), "prefetch added target catalog failed");
                    failed_target_keys.insert(region_key);
                }
            }
        }

        let Some(root_html) = root_html else {
            continue;
        };
        let root_regions = parse_regions(&fid, &root_html);
        let direct_configs = parse_configs(&fid, None, &root_html);
        let root_configs = if root_regions.is_empty() {
            if direct_configs.is_empty() {
                failed_target_keys.insert(root_key.clone());
                continue;
            }
            direct_configs
        } else if direct_configs.is_empty() {
            Vec::new()
        } else {
            // Keep root-page packages we can still prove locally and let failed region fetches
            // mark the country summary as partial instead of erasing successful root results.
            retain_country_direct_configs(direct_configs, &region_configs)
        };
        let region_notice = parse_region_notice(&root_html);
        if let Err(err) = db::apply_catalog_url_fetch_success(
            &state.db,
            &fid,
            None,
            &root_key,
            &root_url,
            root_configs,
            db::CatalogUrlFetchHints {
                region_notice: region_notice.as_deref(),
                empty_result_authoritative: !root_regions.is_empty(),
            },
        )
        .await
        {
            warn!(error = %err, fid, gid = ?Option::<String>::None, "prefetch added target catalog failed");
            failed_target_keys.insert(root_key);
        }
    }

    for (fid, gid) in build_added_topology_targets(added_countries, added_regions) {
        if gid.is_none() || added_country_ids.contains(&fid) {
            continue;
        }

        let gid = gid.expect("guarded by continue");
        let url = format!("{}?fid={fid}&gid={gid}", state.config.upstream_cart_url);
        let url_key = catalog_region_key(&fid, Some(gid.as_str()));

        match upstream
            .fetch_region_configs_detailed(&fid, Some(gid.as_str()))
            .await
        {
            Ok(fetch) => {
                if let Err(err) = db::apply_catalog_url_fetch_success(
                    &state.db,
                    &fid,
                    Some(gid.as_str()),
                    &url_key,
                    &url,
                    fetch.configs,
                    db::CatalogUrlFetchHints {
                        region_notice: fetch.region_notice.as_deref(),
                        empty_result_authoritative: fetch.empty_result_authoritative,
                    },
                )
                .await
                {
                    warn!(error = %err, fid, gid = ?Some(gid.clone()), "prefetch added target catalog failed");
                    failed_target_keys.insert(url_key);
                }
            }
            Err(err) => {
                warn!(error = %err, fid, gid = ?Some(gid.clone()), "prefetch added target catalog failed");
                failed_target_keys.insert(url_key);
            }
        }
    }

    failed_target_keys
}

fn merge_topology_probe_result(
    previous: &CatalogSnapshot,
    topology: crate::upstream::CatalogTopologySnapshot,
) -> crate::upstream::CatalogTopologySnapshot {
    let mut countries = previous.countries.clone();
    for country in topology.countries.iter().cloned() {
        if let Some(existing) = countries
            .iter_mut()
            .find(|current| current.id == country.id)
        {
            *existing = country;
        } else {
            countries.push(country);
        }
    }

    let mut regions = previous.regions.clone();
    for region in topology.regions.iter().cloned() {
        if let Some(existing) = regions
            .iter_mut()
            .find(|current| current.country_id == region.country_id && current.id == region.id)
        {
            *existing = region;
        } else {
            regions.push(region);
        }
    }

    let mut region_notices = previous.region_notices.clone();
    for notice in topology.region_notices.iter().cloned() {
        if let Some(existing) = region_notices.iter_mut().find(|current| {
            current.country_id == notice.country_id && current.region_id == notice.region_id
        }) {
            *existing = notice;
        } else {
            region_notices.push(notice);
        }
    }

    let mut region_notice_initialized_keys = previous.region_notice_initialized_keys.clone();
    region_notice_initialized_keys.extend(topology.region_notice_initialized_keys.iter().cloned());

    crate::upstream::CatalogTopologySnapshot {
        countries,
        regions,
        region_notices,
        region_notice_initialized_keys,
        ambiguous_country_ids: topology.ambiguous_country_ids,
        refreshed_at: topology.refreshed_at,
        request_count: topology.request_count,
    }
}

fn collect_topology_notification_changes(
    previous: &CatalogSnapshot,
    next_countries: &[crate::models::Country],
    next_regions: &[crate::models::Region],
) -> TopologyNotificationChanges {
    let previous_country_ids = previous
        .countries
        .iter()
        .map(|country| country.id.as_str())
        .collect::<HashSet<_>>();
    let next_country_ids = next_countries
        .iter()
        .map(|country| country.id.as_str())
        .collect::<HashSet<_>>();
    let previous_country_names = previous
        .countries
        .iter()
        .map(|country| (country.id.as_str(), country.name.as_str()))
        .collect::<HashMap<_, _>>();
    let next_country_names = next_countries
        .iter()
        .map(|country| (country.id.as_str(), country.name.as_str()))
        .collect::<HashMap<_, _>>();

    let mut changes = TopologyNotificationChanges {
        added_countries: next_countries
            .iter()
            .filter(|country| !previous_country_ids.contains(country.id.as_str()))
            .map(|country| crate::ops::CountryTopologyChange {
                id: country.id.clone(),
                name: country.name.clone(),
            })
            .collect(),
        removed_countries: previous
            .countries
            .iter()
            .filter(|country| !next_country_ids.contains(country.id.as_str()))
            .map(|country| crate::ops::CountryTopologyChange {
                id: country.id.clone(),
                name: country.name.clone(),
            })
            .collect(),
        ..TopologyNotificationChanges::default()
    };

    let previous_region_keys = previous
        .regions
        .iter()
        .map(|region| catalog_region_key(&region.country_id, Some(region.id.as_str())))
        .collect::<HashSet<_>>();
    let next_region_keys = next_regions
        .iter()
        .map(|region| catalog_region_key(&region.country_id, Some(region.id.as_str())))
        .collect::<HashSet<_>>();

    changes.added_regions = next_regions
        .iter()
        .filter(|region| {
            !previous_region_keys.contains(&catalog_region_key(
                &region.country_id,
                Some(region.id.as_str()),
            ))
        })
        .map(|region| crate::ops::RegionTopologyChange {
            country_id: region.country_id.clone(),
            country_name: next_country_names
                .get(region.country_id.as_str())
                .copied()
                .unwrap_or(region.country_id.as_str())
                .to_string(),
            region_id: region.id.clone(),
            region_name: region.name.clone(),
        })
        .collect();

    changes.removed_regions = previous
        .regions
        .iter()
        .filter(|region| {
            !next_region_keys.contains(&catalog_region_key(
                &region.country_id,
                Some(region.id.as_str()),
            ))
        })
        .map(|region| crate::ops::RegionTopologyChange {
            country_id: region.country_id.clone(),
            country_name: previous_country_names
                .get(region.country_id.as_str())
                .copied()
                .unwrap_or(region.country_id.as_str())
                .to_string(),
            region_id: region.id.clone(),
            region_name: region.name.clone(),
        })
        .collect();

    changes.added_countries.sort_by(|a, b| a.id.cmp(&b.id));
    changes.removed_countries.sort_by(|a, b| a.id.cmp(&b.id));
    changes.added_regions.sort_by(|a, b| {
        (a.country_id.as_str(), a.region_id.as_str())
            .cmp(&(b.country_id.as_str(), b.region_id.as_str()))
    });
    changes.removed_regions.sort_by(|a, b| {
        (a.country_id.as_str(), a.region_id.as_str())
            .cmp(&(b.country_id.as_str(), b.region_id.as_str()))
    });

    changes
}

async fn run(state: AppState) -> anyhow::Result<()> {
    let mut next_due: HashMap<String, OffsetDateTime> = HashMap::new();
    let mut last_cleanup: Option<OffsetDateTime> = None;
    let mut next_topology_due: Option<OffsetDateTime> = None;
    let mut next_topology_probe_due: Option<OffsetDateTime> = None;
    let mut next_discovery_due: Option<OffsetDateTime> = None;

    loop {
        let users = sqlx::query("SELECT id FROM users")
            .fetch_all(&state.db)
            .await?
            .into_iter()
            .map(|r| r.get::<String, _>(0))
            .collect::<Vec<_>>();

        let now = OffsetDateTime::now_utc();
        let topology_interval =
            time::Duration::hours(FIXED_CATALOG_TOPOLOGY_REFRESH_INTERVAL_HOURS);
        let topology_probe_interval =
            time::Duration::minutes(FIXED_CATALOG_TOPOLOGY_PROBE_INTERVAL_MINUTES);

        if next_topology_due.is_none()
            || next_topology_probe_due.is_none()
            || next_discovery_due.is_none()
        {
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
            if next_topology_probe_due.is_none() {
                next_topology_probe_due = Some(now.saturating_add(topology_probe_interval));
            }
            if next_discovery_due.is_none() {
                next_discovery_due = Some(if has_topology || has_known_targets {
                    now.saturating_add(time::Duration::seconds(DISCOVERY_INTERVAL_SECONDS))
                } else {
                    now
                });
            }
        }

        let mut topology_job_ran = false;
        if next_topology_due.is_some_and(|due| now >= due) {
            topology_job_ran = true;
            match refresh_catalog_topology(&state, "topology_refresh").await {
                Ok(_) => {
                    next_topology_due = Some(now.saturating_add(topology_interval));
                    next_topology_probe_due = Some(now.saturating_add(topology_probe_interval));
                }
                Err(err) => {
                    warn!(error = %err, "topology refresh failed");
                    next_topology_due =
                        Some(now.saturating_add(time::Duration::seconds(TOPOLOGY_RETRY_SECONDS)));
                }
            }
        }

        if !topology_job_ran && next_topology_probe_due.is_some_and(|due| now >= due) {
            match probe_catalog_topology(&state, "topology_probe").await {
                Ok(_) => {
                    next_topology_probe_due = Some(now.saturating_add(topology_probe_interval));
                }
                Err(err) => {
                    warn!(error = %err, "topology probe failed");
                    next_topology_probe_due =
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
            if let Err(err) = crate::lazycat::maybe_spawn_due_sync(&state, &user_id).await {
                warn!(user_id, error = %err, "lazycat sync scheduling failed");
            }
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
            let _ = db::cleanup_notification_records(
                &state.db,
                state.config.notification_retention_days,
                state.config.notification_retention_max_rows,
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

fn preserve_ambiguous_country_regions(
    topology: &mut crate::upstream::CatalogTopologySnapshot,
    previous: &CatalogSnapshot,
) -> Vec<String> {
    let mut preserved_countries = Vec::new();

    for country_id in topology.ambiguous_country_ids.iter() {
        let previous_regions = previous
            .regions
            .iter()
            .filter(|region| region.country_id == *country_id)
            .cloned()
            .collect::<Vec<_>>();
        if previous_regions.is_empty() {
            continue;
        }

        preserved_countries.push(country_id.clone());
        let mut existing_region_ids = topology
            .regions
            .iter()
            .filter(|region| region.country_id == *country_id)
            .map(|region| region.id.clone())
            .collect::<HashSet<_>>();
        let mut preserved_url_keys = HashSet::from([catalog_region_key(country_id.as_str(), None)]);

        for region in previous_regions {
            let url_key = catalog_region_key(&region.country_id, Some(region.id.as_str()));
            preserved_url_keys.insert(url_key);
            if existing_region_ids.insert(region.id.clone()) {
                topology.regions.push(region);
            }
        }

        topology.region_notice_initialized_keys.extend(
            previous
                .region_notice_initialized_keys
                .iter()
                .filter(|key| preserved_url_keys.contains(*key))
                .cloned(),
        );
        for notice in previous
            .region_notices
            .iter()
            .filter(|notice| notice.country_id == *country_id)
        {
            let notice_key = catalog_region_key(&notice.country_id, notice.region_id.as_deref());
            if preserved_url_keys.contains(&notice_key)
                && !topology.region_notices.iter().any(|current| {
                    current.country_id == notice.country_id && current.region_id == notice.region_id
                })
            {
                topology.region_notices.push(notice.clone());
            }
        }
    }

    preserved_countries
}

async fn apply_topology_snapshot(
    catalog: &tokio::sync::RwLock<CatalogSnapshot>,
    topology: crate::upstream::CatalogTopologySnapshot,
    source_url: &str,
) {
    let active_keys = topology
        .countries
        .iter()
        .map(|country| Some((country.id.clone(), None)))
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
            .enqueue_and_wait_for_poller(&fid, gid.as_deref(), user_id)
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

                let item = db::load_notification_record_item_snapshot(&state.db, &id)
                    .await?
                    .ok_or_else(|| {
                        anyhow::anyhow!("missing notification snapshot for config {id}")
                    })?;
                let partition_label = item.region_name.as_ref().map_or_else(
                    || Some(item.country_name.clone()),
                    |region_name| Some(format!("{} / {}", item.country_name, region_name)),
                );
                let record_id = db::insert_notification_record(
                    &state.db,
                    user_id,
                    &crate::models::NotificationRecordDraft {
                        kind: format!("monitoring.{}", events.join("+")),
                        title: notification
                            .as_ref()
                            .expect("notification exists when events exist")
                            .title
                            .clone(),
                        summary: notification
                            .as_ref()
                            .expect("notification exists when events exist")
                            .summary
                            .clone(),
                        partition_label,
                        telegram_status: if settings.telegram_enabled {
                            "pending".to_string()
                        } else {
                            "skipped".to_string()
                        },
                        web_push_status: "skipped".to_string(),
                        items: vec![item],
                    },
                )
                .await?;
                let telegram_text = notification_content::append_notification_record_link(
                    &notification
                        .as_ref()
                        .expect("notification exists when events exist")
                        .telegram_text,
                    settings.site_base_url.as_deref(),
                    &record_id,
                );

                if settings.telegram_enabled {
                    let token = settings
                        .telegram_bot_token
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty());
                    if let Some(token) = token.filter(|_| !settings.telegram_targets.is_empty()) {
                        let deliveries = crate::notifications::send_telegram_to_targets(
                            &state.config.telegram_api_base_url,
                            token,
                            &settings.telegram_targets,
                            &telegram_text,
                        )
                        .await;
                        db::replace_notification_record_deliveries(
                            &state.db,
                            &record_id,
                            "telegram",
                            &deliveries,
                        )
                        .await?;
                        let status = db::aggregate_telegram_status(true, &deliveries);
                        db::update_notification_record_channel_status(
                            &state.db, &record_id, "telegram", &status,
                        )
                        .await?;
                        for delivery in &deliveries {
                            let result = if delivery.status == "success" {
                                "success"
                            } else {
                                "error"
                            };
                            let _ = state
                                .ops
                                .record_notify(
                                    run.run_id,
                                    "telegram",
                                    result,
                                    delivery.error.as_deref(),
                                )
                                .await;
                            if let Some(err) = delivery.error.as_deref() {
                                warn!(user_id, target = %delivery.target, error = %err, "telegram send failed");
                                db::insert_log(
                                    &state.db,
                                    Some(user_id),
                                    "warn",
                                    "notify.telegram",
                                    "telegram send failed",
                                    Some(serde_json::json!({
                                        "target": delivery.target,
                                        "error": err,
                                    })),
                                )
                                .await?;
                            }
                        }
                    } else {
                        let deliveries = vec![crate::models::NotificationRecordDeliveryView {
                            channel: "telegram".to_string(),
                            target: "(config)".to_string(),
                            status: "error".to_string(),
                            error: Some("missing telegram config".to_string()),
                        }];
                        db::replace_notification_record_deliveries(
                            &state.db,
                            &record_id,
                            "telegram",
                            &deliveries,
                        )
                        .await?;
                        db::update_notification_record_channel_status(
                            &state.db, &record_id, "telegram", "error",
                        )
                        .await?;
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
                            "notificationRecordId": record_id,
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
    fn merge_topology_probe_result_keeps_previous_targets_and_adds_new_ones() {
        let previous = CatalogSnapshot {
            countries: vec![crate::models::Country {
                id: "2".to_string(),
                name: "CN".to_string(),
            }],
            regions: vec![crate::models::Region {
                id: "56".to_string(),
                country_id: "2".to_string(),
                name: "HKG Premium".to_string(),
                location_name: Some("湾仔".to_string()),
            }],
            region_notices: vec![crate::models::RegionNotice {
                country_id: "2".to_string(),
                region_id: Some("56".to_string()),
                text: "old notice".to_string(),
            }],
            region_notice_initialized_keys: HashSet::from([String::from("2:56")]),
            configs: Vec::new(),
            fetched_at: "2026-01-01T00:00:00Z".to_string(),
            source_url: "https://example.invalid/cart".to_string(),
            topology_refreshed_at: Some("2026-01-01T00:00:00Z".to_string()),
            topology_request_count: 0,
            topology_status: "success".to_string(),
            topology_message: None,
        };

        let topology = crate::upstream::CatalogTopologySnapshot {
            countries: vec![crate::models::Country {
                id: "2".to_string(),
                name: "CN".to_string(),
            }],
            regions: vec![crate::models::Region {
                id: "57".to_string(),
                country_id: "2".to_string(),
                name: "LAX Pro".to_string(),
                location_name: Some("洛杉矶".to_string()),
            }],
            region_notices: vec![crate::models::RegionNotice {
                country_id: "2".to_string(),
                region_id: Some("57".to_string()),
                text: "new notice".to_string(),
            }],
            region_notice_initialized_keys: HashSet::from([String::from("2:57")]),
            ambiguous_country_ids: HashSet::new(),
            refreshed_at: "2026-01-01T00:15:00Z".to_string(),
            request_count: 2,
        };

        let merged = merge_topology_probe_result(&previous, topology);
        let target_keys = merged
            .regions
            .iter()
            .map(|region| catalog_region_key(&region.country_id, Some(region.id.as_str())))
            .collect::<HashSet<_>>();

        assert_eq!(
            target_keys,
            HashSet::from([String::from("2:56"), String::from("2:57")])
        );
        assert!(
            merged
                .region_notices
                .iter()
                .any(|notice| notice.region_id.as_deref() == Some("56")
                    && notice.text == "old notice")
        );
        assert!(
            merged
                .region_notices
                .iter()
                .any(|notice| notice.region_id.as_deref() == Some("57")
                    && notice.text == "new notice")
        );
        assert!(merged.region_notice_initialized_keys.contains("2:56"));
        assert!(merged.region_notice_initialized_keys.contains("2:57"));
    }

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
