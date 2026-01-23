use crate::app::{json_forbidden, json_invalid_argument};
use crate::models::*;
use crate::{app::AppState, db};
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{get, patch, post},
    Json, Router,
};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

type RefreshTask = ((String, Option<String>), Vec<String>);

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(api_health))
        .route("/bootstrap", get(get_bootstrap))
        .route("/products", get(get_products))
        .route("/inventory/history", post(post_inventory_history))
        .route("/refresh", post(post_refresh))
        .route("/refresh/status", get(get_refresh_status))
        .route("/monitoring", get(get_monitoring))
        .route(
            "/monitoring/configs/:config_id",
            patch(patch_monitoring_config),
        )
        .route("/settings", get(get_settings).put(put_settings))
        .route("/logs", get(get_logs))
        .route(
            "/notifications/web-push/subscriptions",
            post(post_web_push_subscription),
        )
        .with_state(state.clone())
        .layer(axum::middleware::from_fn_with_state(
            state,
            enforce_same_origin,
        ))
}

async fn api_health(
    State(state): State<AppState>,
    user: axum::extract::Extension<UserView>,
) -> Response<Body> {
    let _ = db::ensure_user(&state.db, &state.config, &user.0.id).await;
    Json(serde_json::json!({
        "status": "ok",
        "version": state.config.effective_version,
    }))
    .into_response()
}

async fn enforce_same_origin(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response<Body> {
    let headers = req.headers();
    let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else {
        return next.run(req).await;
    };

    // Whether this deployment trusts proxy-injected headers. If we already rely on a proxy to provide
    // the user identity header, we also treat `x-forwarded-*` as trusted metadata.
    let trust_proxy_headers = state.config.auth_user_header.is_some();

    let host_header = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .unwrap_or_default();

    let forwarded_host = headers
        .get("x-forwarded-host")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            v.split(',')
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .next_back()
        });

    let expected_host = if trust_proxy_headers {
        forwarded_host.unwrap_or(host_header)
    } else {
        host_header
    };

    let expected_scheme = if trust_proxy_headers {
        headers
            .get("x-forwarded-proto")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| {
                v.split(',')
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .next_back()
            })
            .unwrap_or("http")
    } else {
        req.uri().scheme_str().unwrap_or("http")
    };

    // Compare Origin scheme + authority (host[:port]) to the effective external scheme + Host.
    let origin = origin.trim();
    let Ok(origin_uri) = origin.parse::<axum::http::Uri>() else {
        return json_forbidden().into_response();
    };
    let Some(origin_scheme) = origin_uri.scheme_str() else {
        return json_forbidden().into_response();
    };
    let Some(origin_authority) = origin_uri.authority() else {
        return json_forbidden().into_response();
    };
    if !origin_authority
        .as_str()
        .eq_ignore_ascii_case(expected_host)
        || !origin_scheme.eq_ignore_ascii_case(expected_scheme)
    {
        return json_forbidden().into_response();
    }

    next.run(req).await
}

fn json_rate_limited() -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::TOO_MANY_REQUESTS,
        Json(ErrorResponse {
            error: ErrorInfo {
                code: "RATE_LIMITED",
                message: "刷新太频繁，请稍后再试",
            },
        }),
    )
}

fn json_internal_error() -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            error: ErrorInfo {
                code: "INTERNAL",
                message: "Internal error",
            },
        }),
    )
}

fn parse_lc_region(id: &str) -> Option<(String, Option<String>)> {
    let mut it = id.split(':');
    if it.next()? != "lc" {
        return None;
    }
    let fid = it.next()?.to_string();
    let gid = it.next()?;
    let gid = if gid == "0" {
        None
    } else {
        Some(gid.to_string())
    };
    Some((fid, gid))
}

fn refresh_status_idle() -> RefreshStatusResponse {
    RefreshStatusResponse {
        state: "idle".to_string(),
        done: 0,
        total: 0,
        message: None,
    }
}

async fn get_refresh_status(
    State(state): State<AppState>,
    user: axum::extract::Extension<UserView>,
) -> Result<Json<RefreshStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    let user_id = user.0.id.clone();
    let status = state
        .manual_refresh_status
        .lock()
        .await
        .get(&user_id)
        .cloned()
        .unwrap_or_else(refresh_status_idle);
    Ok(Json(status))
}

async fn post_refresh(
    State(state): State<AppState>,
    user: axum::extract::Extension<UserView>,
) -> Result<Json<RefreshStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    const MIN_INTERVAL_SECONDS: i64 = 30;

    let user_id = user.0.id.clone();
    let now = OffsetDateTime::now_utc();

    {
        let existing = state
            .manual_refresh_status
            .lock()
            .await
            .get(&user_id)
            .cloned();
        if let Some(st) = existing {
            if st.state == "syncing" {
                return Ok(Json(st));
            }
        }
    }

    {
        let mut gate = state.manual_refresh_gate.lock().await;
        if let Some(last) = gate.get(&user_id) {
            if now - *last < time::Duration::seconds(MIN_INTERVAL_SECONDS) {
                return Err(json_rate_limited());
            }
        }
        gate.insert(user_id.clone(), now);
    }

    let enabled = db::list_enabled_monitoring_config_ids(&state.db, &user_id)
        .await
        .map_err(|_| json_invalid_argument())?;

    let mut full_catalog = enabled.is_empty();
    let mut tasks: Vec<RefreshTask> = Vec::new();
    if !enabled.is_empty() {
        let mut by_region: HashMap<(String, Option<String>), Vec<String>> = HashMap::new();
        for id in enabled.iter() {
            let Some((fid, gid)) = parse_lc_region(id) else {
                continue;
            };
            by_region.entry((fid, gid)).or_default().push(id.clone());
        }
        tasks = by_region.into_iter().collect::<Vec<_>>();
        tasks.sort_by(|a, b| {
            let (a_fid, a_gid) = &a.0;
            let (b_fid, b_gid) = &b.0;
            (a_fid, a_gid.as_deref().unwrap_or("")).cmp(&(b_fid, b_gid.as_deref().unwrap_or("")))
        });
        if tasks.is_empty() {
            full_catalog = true;
        }
    }

    let total = if full_catalog {
        1
    } else {
        tasks.len().max(1) as i64
    };
    let syncing = RefreshStatusResponse {
        state: "syncing".to_string(),
        done: 0,
        total,
        message: None,
    };

    state
        .manual_refresh_status
        .lock()
        .await
        .insert(user_id.clone(), syncing.clone());

    tokio::spawn({
        let state = state.clone();
        let user_id = user_id.clone();
        let enabled = enabled.clone();
        async move {
            run_refresh_job(state, &user_id, &enabled, full_catalog, tasks).await;
        }
    });

    Ok(Json(syncing))
}

async fn run_refresh_job(
    state: AppState,
    user_id: &str,
    _enabled: &[String],
    full_catalog: bool,
    tasks: Vec<RefreshTask>,
) {
    let total = if full_catalog {
        1
    } else {
        tasks.len().max(1) as i64
    };
    let mut done: i64 = 0;

    let res: anyhow::Result<()> = async {
        let upstream =
            crate::upstream::UpstreamClient::new(state.config.upstream_cart_url.clone())?;

        if full_catalog {
            let catalog = upstream.fetch_catalog().await?;
            db::upsert_catalog_configs(&state.db, &catalog.configs).await?;
            *state.catalog.write().await = catalog;
            done = 1;
            return Ok(());
        }

        for ((fid, gid), _ids) in tasks.iter() {
            let parsed = upstream.fetch_region_configs(fid, gid.as_deref()).await?;

            let parsed_map = parsed
                .into_iter()
                .map(|c| (c.id.clone(), c))
                .collect::<HashMap<_, _>>();

            if !parsed_map.is_empty() {
                let changed = parsed_map.values().cloned().collect::<Vec<_>>();
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
            }

            done += 1;
            state.manual_refresh_status.lock().await.insert(
                user_id.to_string(),
                RefreshStatusResponse {
                    state: "syncing".to_string(),
                    done,
                    total,
                    message: None,
                },
            );
        }

        Ok(())
    }
    .await;

    let next = match res {
        Ok(()) => RefreshStatusResponse {
            state: "success".to_string(),
            done,
            total,
            message: None,
        },
        Err(_) => RefreshStatusResponse {
            state: "error".to_string(),
            done,
            total,
            message: Some("上游抓取失败".to_string()),
        },
    };

    state
        .manual_refresh_status
        .lock()
        .await
        .insert(user_id.to_string(), next);
}

async fn get_bootstrap(
    State(state): State<AppState>,
    user: axum::extract::Extension<UserView>,
) -> Result<Json<BootstrapResponse>, (StatusCode, Json<ErrorResponse>)> {
    let user_id = user.0.id.clone();
    let settings = db::ensure_user(&state.db, &state.config, &user_id)
        .await
        .map_err(|_| json_invalid_argument())?;
    let enabled_config_ids = db::list_enabled_monitoring_config_ids(&state.db, &user_id)
        .await
        .map_err(|_| json_invalid_argument())?;

    let snapshot = state.catalog.read().await.clone();
    let configs = snapshot
        .configs
        .iter()
        .map(|c| snapshot.to_view(c, enabled_config_ids.iter().any(|id| id == &c.id)))
        .collect::<Vec<_>>();

    let poll_interval_seconds = settings.poll_interval_minutes * 60;
    let monitoring = MonitoringView {
        enabled_config_ids: enabled_config_ids.clone(),
        poll: MonitoringPollView {
            interval_seconds: poll_interval_seconds,
            jitter_pct: settings.poll_jitter_pct,
        },
    };

    let settings_view = settings.to_view(state.config.web_push_vapid_public_key.clone());

    Ok(Json(BootstrapResponse {
        user: UserView {
            id: user_id,
            display_name: None,
        },
        catalog: CatalogView {
            countries: snapshot.countries,
            regions: snapshot.regions,
            configs,
            fetched_at: snapshot.fetched_at,
            source: CatalogSource {
                url: snapshot.source_url,
            },
        },
        monitoring,
        settings: settings_view,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProductsQuery {
    country_id: Option<String>,
    region_id: Option<String>,
}

async fn get_products(
    State(state): State<AppState>,
    user: axum::extract::Extension<UserView>,
    Query(q): Query<ProductsQuery>,
) -> Result<Json<ProductsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let enabled_config_ids = db::list_enabled_monitoring_config_ids(&state.db, &user.0.id)
        .await
        .map_err(|_| json_invalid_argument())?;
    let snapshot = state.catalog.read().await.clone();

    let configs = snapshot
        .configs
        .iter()
        .filter(|c| {
            (q.country_id.as_ref().is_none()
                || q.country_id.as_ref().is_some_and(|id| &c.country_id == id))
                && (q.region_id.as_ref().is_none()
                    || q.region_id
                        .as_ref()
                        .is_some_and(|id| c.region_id.as_ref() == Some(id)))
        })
        .map(|c| snapshot.to_view(c, enabled_config_ids.iter().any(|id| id == &c.id)))
        .collect::<Vec<_>>();

    Ok(Json(ProductsResponse {
        configs,
        fetched_at: snapshot.fetched_at,
    }))
}

async fn post_inventory_history(
    State(state): State<AppState>,
    _user: axum::extract::Extension<UserView>,
    Json(req): Json<InventoryHistoryRequest>,
) -> Result<Json<InventoryHistoryResponse>, (StatusCode, Json<ErrorResponse>)> {
    const MAX_CONFIG_IDS: usize = 200;

    let mut ids: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for id in req.config_ids {
        let id = id.trim().to_string();
        if id.is_empty() {
            continue;
        }
        if seen.insert(id.clone()) {
            ids.push(id);
        }
        if ids.len() > MAX_CONFIG_IDS {
            return Err(json_invalid_argument());
        }
    }
    if ids.is_empty() {
        return Err(json_invalid_argument());
    }

    let now = OffsetDateTime::now_utc();
    let to = now
        .replace_second(0)
        .ok()
        .and_then(|t| t.replace_nanosecond(0).ok())
        .unwrap_or(now);
    let from = to.saturating_sub(time::Duration::hours(24));

    let to_s = to
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());
    let from_s = from
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());

    let rows = db::list_inventory_samples_1m(&state.db, &ids, &from_s, &to_s)
        .await
        .map_err(|_| json_internal_error())?;

    let mut by_id: HashMap<String, Vec<InventoryHistoryPoint>> = HashMap::new();
    for r in rows {
        by_id
            .entry(r.config_id)
            .or_default()
            .push(InventoryHistoryPoint {
                ts_minute: r.ts_minute,
                quantity: r.inventory_quantity,
            });
    }

    let series = ids
        .iter()
        .map(|id| InventoryHistorySeries {
            config_id: id.clone(),
            points: by_id.remove(id).unwrap_or_default(),
        })
        .collect::<Vec<_>>();

    Ok(Json(InventoryHistoryResponse {
        window: InventoryHistoryWindow {
            from: from_s,
            to: to_s,
        },
        series,
    }))
}

async fn get_monitoring(
    State(state): State<AppState>,
    user: axum::extract::Extension<UserView>,
) -> Result<Json<MonitoringListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let enabled = db::list_enabled_monitoring_config_ids(&state.db, &user.0.id)
        .await
        .map_err(|_| json_invalid_argument())?;
    let enabled_set: std::collections::HashSet<_> = enabled.into_iter().collect();

    let snapshot = state.catalog.read().await.clone();
    let items = snapshot
        .configs
        .iter()
        .filter(|c| enabled_set.contains(&c.id))
        .map(|c| snapshot.to_view(c, true))
        .collect::<Vec<_>>();

    Ok(Json(MonitoringListResponse {
        items,
        fetched_at: snapshot.fetched_at,
    }))
}

async fn patch_monitoring_config(
    State(state): State<AppState>,
    user: axum::extract::Extension<UserView>,
    Path(config_id): Path<String>,
    Json(req): Json<MonitoringToggleRequest>,
) -> Result<Json<MonitoringToggleResponse>, (StatusCode, Json<ErrorResponse>)> {
    let snapshot = state.catalog.read().await;
    let Some(config) = snapshot.configs.iter().find(|c| c.id == config_id) else {
        return Err(json_invalid_argument());
    };
    if !config.monitor_supported {
        return Err(json_invalid_argument());
    }
    drop(snapshot);

    db::set_monitoring_config_enabled(&state.db, &user.0.id, &config_id, req.enabled)
        .await
        .map_err(|_| json_invalid_argument())?;

    Ok(Json(MonitoringToggleResponse {
        config_id,
        enabled: req.enabled,
    }))
}

async fn get_settings(
    State(state): State<AppState>,
    user: axum::extract::Extension<UserView>,
) -> Result<Json<SettingsView>, (StatusCode, Json<ErrorResponse>)> {
    let settings = db::ensure_user(&state.db, &state.config, &user.0.id)
        .await
        .map_err(|_| json_invalid_argument())?;
    Ok(Json(
        settings.to_view(state.config.web_push_vapid_public_key.clone()),
    ))
}

async fn put_settings(
    State(state): State<AppState>,
    user: axum::extract::Extension<UserView>,
    Json(req): Json<SettingsUpdateRequest>,
) -> Result<Json<SettingsView>, (StatusCode, Json<ErrorResponse>)> {
    if req.poll.interval_minutes < 1 || !(0.0..=1.0).contains(&req.poll.jitter_pct) {
        return Err(json_invalid_argument());
    }

    let settings = db::update_settings(&state.db, &user.0.id, req)
        .await
        .map_err(|_| json_invalid_argument())?;
    Ok(Json(
        settings.to_view(state.config.web_push_vapid_public_key.clone()),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogsQuery {
    level: Option<String>,
    cursor: Option<String>,
    limit: Option<i64>,
}

async fn get_logs(
    State(state): State<AppState>,
    user: axum::extract::Extension<UserView>,
    Query(q): Query<LogsQuery>,
) -> Result<Json<LogsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let (items, next_cursor) = db::list_logs(
        &state.db,
        &user.0.id,
        q.level.as_deref(),
        q.cursor.as_deref(),
        limit,
    )
    .await
    .map_err(|_| json_invalid_argument())?;
    Ok(Json(LogsResponse { items, next_cursor }))
}

async fn post_web_push_subscription(
    State(state): State<AppState>,
    user: axum::extract::Extension<UserView>,
    Json(req): Json<WebPushSubscribeRequest>,
) -> Result<Json<WebPushSubscribeResponse>, (StatusCode, Json<ErrorResponse>)> {
    let id = db::insert_web_push_subscription(&state.db, &user.0.id, req)
        .await
        .map_err(|_| json_invalid_argument())?;
    Ok(Json(WebPushSubscribeResponse {
        subscription_id: id,
    }))
}
