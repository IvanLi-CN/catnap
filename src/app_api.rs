use crate::app::{json_forbidden, json_invalid_argument};
use crate::models::*;
use crate::{app::AppState, db};
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, Request, StatusCode},
    middleware::Next,
    response::sse::{Event, KeepAlive, Sse},
    response::{IntoResponse, Response},
    routing::{get, patch, post},
    Json, Router,
};
use futures_util::stream;
use futures_util::StreamExt;
use serde::Deserialize;
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::net::IpAddr;
use std::time::Duration;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tracing::warn;

const WEB_DIST_BUILD_ID: &str = env!("CATNAP_WEB_DIST_BUILD_ID");

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(api_health))
        .route("/meta", get(get_meta))
        .route("/update", get(get_update))
        .route("/bootstrap", get(get_bootstrap))
        .route("/products", get(get_products))
        .route("/inventory/history", post(post_inventory_history))
        .route("/catalog/refresh", post(post_catalog_refresh))
        .route("/catalog/refresh/events", get(get_catalog_refresh_events))
        .route("/ops/state", get(get_ops_state))
        .route("/ops/stream", get(get_ops_stream))
        .route("/refresh", post(post_refresh))
        .route("/refresh/status", get(get_refresh_status))
        .route("/monitoring", get(get_monitoring))
        .route(
            "/monitoring/configs/:config_id",
            patch(patch_monitoring_config),
        )
        .route("/settings", get(get_settings).put(put_settings))
        .route("/logs", get(get_logs))
        .route("/notifications/telegram/test", post(post_telegram_test))
        .route(
            "/notifications/web-push/subscriptions",
            post(post_web_push_subscription),
        )
        .route("/notifications/web-push/test", post(post_web_push_test))
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

async fn get_meta(
    State(state): State<AppState>,
    _user: axum::extract::Extension<UserView>,
) -> Json<AppMetaView> {
    Json(AppMetaView {
        effective_version: state.config.effective_version,
        web_dist_build_id: WEB_DIST_BUILD_ID.to_string(),
        repo_url: state.config.repo_url,
    })
}

async fn get_update(
    State(state): State<AppState>,
    _user: axum::extract::Extension<UserView>,
) -> Json<UpdateCheckResponse> {
    Json(state.update_checker.check().await)
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
        .and_then(|v| v.split(',').map(str::trim).rfind(|v| !v.is_empty()));

    let expected_host = if trust_proxy_headers {
        forwarded_host.unwrap_or(host_header)
    } else {
        host_header
    };

    let expected_scheme = if trust_proxy_headers {
        headers
            .get("x-forwarded-proto")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').map(str::trim).rfind(|v| !v.is_empty()))
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
                message: "刷新太频繁，请稍后再试".to_string(),
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
                message: "Internal error".to_string(),
            },
        }),
    )
}

fn json_invalid_argument_with_message(
    message: impl Into<String>,
) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse {
            error: ErrorInfo {
                code: "INVALID_ARGUMENT",
                message: message.into(),
            },
        }),
    )
}

fn json_internal_error_with_message(
    message: impl Into<String>,
) -> (StatusCode, Json<ErrorResponse>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            error: ErrorInfo {
                code: "INTERNAL",
                message: message.into(),
            },
        }),
    )
}

fn legacy_refresh_status_from_catalog(st: &CatalogRefreshStatus) -> RefreshStatusResponse {
    let state = match st.state.as_str() {
        "running" => "syncing",
        "success" => "success",
        "error" => "error",
        _ => "idle",
    };
    RefreshStatusResponse {
        state: state.to_string(),
        done: st.done,
        total: st.total,
        message: st.message.clone(),
    }
}

async fn post_catalog_refresh(
    State(state): State<AppState>,
    user: axum::extract::Extension<UserView>,
) -> Result<Json<CatalogRefreshStatus>, (StatusCode, Json<ErrorResponse>)> {
    let user_id = user.0.id.clone();
    match state
        .catalog_refresh
        .trigger(
            state.clone(),
            crate::catalog_refresh::RefreshTrigger::Manual,
            Some(&user_id),
        )
        .await
    {
        Ok(st) => Ok(Json(st)),
        Err(crate::catalog_refresh::TriggerError::RateLimited) => Err(json_rate_limited()),
        Err(crate::catalog_refresh::TriggerError::Internal(_)) => Err(json_internal_error()),
    }
}

async fn get_catalog_refresh_events(
    State(state): State<AppState>,
    _user: axum::extract::Extension<UserView>,
) -> Sse<impl futures_util::stream::Stream<Item = Result<Event, Infallible>>> {
    let initial = state.catalog_refresh.status().await;
    let rx = state.catalog_refresh.subscribe();

    let initial_stream = stream::once(async move {
        let data = serde_json::to_string(&initial).unwrap_or_else(|_| "{}".to_string());
        Ok::<_, Infallible>(Event::default().event("catalog.refresh").data(data))
    });

    let updates_stream = stream::unfold(rx, |mut rx| async {
        loop {
            match rx.recv().await {
                Ok(st) => {
                    let data = serde_json::to_string(&st).unwrap_or_else(|_| "{}".to_string());
                    return Some((Ok(Event::default().event("catalog.refresh").data(data)), rx));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
            }
        }
    });

    Sse::new(initial_stream.chain(updates_stream)).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpsStateQuery {
    range: Option<String>,
    log_limit: Option<i64>,
    task_limit: Option<i64>,
}

async fn get_ops_state(
    State(state): State<AppState>,
    _user: axum::extract::Extension<UserView>,
    Query(q): Query<OpsStateQuery>,
) -> Result<Json<crate::ops::OpsStateSnapshot>, (StatusCode, Json<ErrorResponse>)> {
    let range = match q.range.as_deref().map(crate::ops::OpsRange::parse) {
        Some(Some(r)) => r,
        Some(None) => return Err(json_invalid_argument()),
        None => crate::ops::OpsRange::H24,
    };
    if q.log_limit.is_some_and(|v| !(1..=500).contains(&v))
        || q.task_limit.is_some_and(|v| !(1..=500).contains(&v))
    {
        return Err(json_invalid_argument());
    }
    let snap = state
        .ops
        .snapshot(range, q.log_limit, q.task_limit)
        .await
        .map_err(|_| json_internal_error())?;
    Ok(Json(snap))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpsStreamQuery {
    range: Option<String>,
}

async fn get_ops_stream(
    State(state): State<AppState>,
    _user: axum::extract::Extension<UserView>,
    Query(q): Query<OpsStreamQuery>,
    headers: axum::http::HeaderMap,
) -> Result<
    Sse<impl futures_util::stream::Stream<Item = Result<Event, Infallible>>>,
    (StatusCode, Json<ErrorResponse>),
> {
    let range = match q.range.as_deref().map(crate::ops::OpsRange::parse) {
        Some(Some(r)) => r,
        Some(None) => return Err(json_invalid_argument()),
        None => crate::ops::OpsRange::H24,
    };

    let now = OffsetDateTime::now_utc();
    let server_time = now
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());
    let replay_window_seconds = state.config.ops_sse_replay_window_seconds.max(1);
    let cutoff = now
        .saturating_sub(time::Duration::seconds(replay_window_seconds))
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());

    let cursor_id = state.ops.cursor_id().await.unwrap_or(0);

    let hello_payload = serde_json::json!({
        "serverTime": server_time,
        "range": range.as_str(),
        "replayWindowSeconds": replay_window_seconds,
    });
    let hello_event = Event::default()
        .id(cursor_id.to_string())
        .event("ops.hello")
        .data(serde_json::to_string(&hello_payload).unwrap_or_else(|_| "{}".to_string()));

    let stats = state
        .ops
        .stats(range, now)
        .await
        .unwrap_or(crate::ops::OpsStatsView {
            collection: crate::ops::OpsRateBucketView {
                total: 0,
                success: 0,
                failure: 0,
                success_rate_pct: 0.0,
            },
            notify: crate::ops::OpsNotifyStatsView {
                telegram: None,
                web_push: None,
            },
        });
    let metrics_payload = serde_json::json!({
        "serverTime": server_time,
        "range": range.as_str(),
        "stats": stats,
    });
    let metrics_event = Event::default()
        .id(cursor_id.to_string())
        .event("ops.metrics")
        .data(serde_json::to_string(&metrics_payload).unwrap_or_else(|_| "{}".to_string()));

    let last_event_id = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty());

    let mut reset_reason: Option<&'static str> = None;
    let mut reset_details: Option<String> = None;
    let mut replay: Vec<crate::ops::StoredOpsEvent> = Vec::new();

    if let Some(raw) = last_event_id {
        match raw.parse::<i64>() {
            Ok(id) if id > 0 => {
                let min_id = state.ops.min_replay_id_since(&cutoff).await.unwrap_or(None);
                match min_id {
                    Some(min_id) if id >= min_id => {
                        replay = state
                            .ops
                            .replay_since(id, &cutoff)
                            .await
                            .unwrap_or_default();
                    }
                    _ => {
                        reset_reason = Some("stale_last_event_id");
                        reset_details = Some(format!("last_event_id={id} cutoff={cutoff}"));
                    }
                }
            }
            _ => {
                reset_reason = Some("invalid_last_event_id");
                reset_details = Some(format!("last_event_id={raw}"));
            }
        }
    }

    let mut initial_items: Vec<Result<Event, Infallible>> = Vec::new();
    initial_items.push(Ok::<_, Infallible>(hello_event));
    initial_items.push(Ok::<_, Infallible>(metrics_event));
    if let Some(reason) = reset_reason {
        let payload = serde_json::json!({
            "serverTime": server_time,
            "reason": reason,
            "details": reset_details,
        });
        initial_items.push(Ok::<_, Infallible>(
            Event::default()
                .id(cursor_id.to_string())
                .event("ops.reset")
                .data(serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string())),
        ));
    }
    for e in replay {
        initial_items.push(Ok::<_, Infallible>(
            Event::default()
                .id(e.id.to_string())
                .event(e.event)
                .data(e.data_json),
        ));
    }
    let initial_stream = stream::iter(initial_items);

    let rx = state.ops.subscribe();

    let updates_stream = stream::unfold(rx, |mut rx| async {
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    let out = Event::default()
                        .id(ev.id.to_string())
                        .event(ev.event)
                        .data(ev.data_json);
                    return Some((Ok::<_, Infallible>(out), rx));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
            }
        }
    });

    let metrics_stream = stream::unfold((state.clone(), range), |(state, range)| async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let now = OffsetDateTime::now_utc();
        let server_time = now
            .format(&Rfc3339)
            .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());
        let stats = state.ops.stats(range, now).await.ok()?;
        let id = state.ops.cursor_id().await.unwrap_or(0);
        let payload = serde_json::json!({
            "serverTime": server_time,
            "range": range.as_str(),
            "stats": stats,
        });
        Some((
            Ok::<_, Infallible>(
                Event::default()
                    .id(id.to_string())
                    .event("ops.metrics")
                    .data(serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string())),
            ),
            (state, range),
        ))
    });

    let combined =
        futures_util::stream::select(initial_stream.chain(updates_stream), metrics_stream);
    Ok(Sse::new(combined).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

async fn get_refresh_status(
    State(state): State<AppState>,
    _user: axum::extract::Extension<UserView>,
) -> Result<Json<RefreshStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    Ok(Json(legacy_refresh_status_from_catalog(
        &state.catalog_refresh.status().await,
    )))
}

async fn post_refresh(
    State(state): State<AppState>,
    user: axum::extract::Extension<UserView>,
) -> Result<Json<RefreshStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    let user_id = user.0.id.clone();
    let st = match state
        .catalog_refresh
        .trigger(
            state.clone(),
            crate::catalog_refresh::RefreshTrigger::Manual,
            Some(&user_id),
        )
        .await
    {
        Ok(st) => st,
        Err(crate::catalog_refresh::TriggerError::RateLimited) => return Err(json_rate_limited()),
        Err(crate::catalog_refresh::TriggerError::Internal(_)) => return Err(json_internal_error()),
    };

    Ok(Json(legacy_refresh_status_from_catalog(&st)))
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

    let configs = db::list_catalog_configs_view(&state.db, &user_id, None, None)
        .await
        .map_err(|_| json_internal_error())?;

    let snapshot = state.catalog.read().await.clone();

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
        app: AppMetaView {
            effective_version: state.config.effective_version.clone(),
            web_dist_build_id: WEB_DIST_BUILD_ID.to_string(),
            repo_url: state.config.repo_url.clone(),
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
    let configs = db::list_catalog_configs_view(
        &state.db,
        &user.0.id,
        q.country_id.as_deref(),
        q.region_id.as_deref(),
    )
    .await
    .map_err(|_| json_internal_error())?;

    let snapshot = state.catalog.read().await.clone();

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
    let items = db::list_monitoring_configs_view(&state.db, &user.0.id)
        .await
        .map_err(|_| json_internal_error())?;
    let recent_listed24h = db::list_recent_listed_24h_view(&state.db, &user.0.id)
        .await
        .map_err(|_| json_internal_error())?;

    let snapshot = state.catalog.read().await.clone();
    Ok(Json(MonitoringListResponse {
        items,
        fetched_at: snapshot.fetched_at,
        recent_listed24h,
    }))
}

async fn patch_monitoring_config(
    State(state): State<AppState>,
    user: axum::extract::Extension<UserView>,
    Path(config_id): Path<String>,
    Json(req): Json<MonitoringToggleRequest>,
) -> Result<Json<MonitoringToggleResponse>, (StatusCode, Json<ErrorResponse>)> {
    let row = sqlx::query("SELECT country_id FROM catalog_configs WHERE id = ?")
        .bind(&config_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| json_internal_error())?;
    let Some(row) = row else {
        return Err(json_invalid_argument());
    };
    let country_id = row.get::<String, _>(0);
    if country_id.trim() == "2" {
        return Err(json_invalid_argument());
    }

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
    if let Some(ref cr) = req.catalog_refresh {
        if let Some(hours) = cr.auto_interval_hours {
            if !(1..=24 * 30).contains(&hours) {
                return Err(json_invalid_argument_with_message(
                    "自动全量刷新间隔（小时）必须在 1..=720，或设为 null 关闭",
                ));
            }
        }
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
    validate_web_push_endpoint(
        req.subscription.endpoint.as_str(),
        state.config.allow_insecure_local_web_push_endpoints,
    )
    .await?;
    let id = db::insert_web_push_subscription(&state.db, &user.0.id, req)
        .await
        .map_err(|_| json_invalid_argument())?;
    Ok(Json(WebPushSubscribeResponse {
        subscription_id: id,
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TelegramTestRequest {
    bot_token: Option<String>,
    target: Option<String>,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WebPushTestRequest {
    title: Option<String>,
    body: Option<String>,
    url: Option<String>,
}

#[derive(Debug, serde::Serialize)]
struct OkResponse {
    ok: bool,
}

async fn post_telegram_test(
    State(state): State<AppState>,
    user: axum::extract::Extension<UserView>,
    Json(req): Json<TelegramTestRequest>,
) -> Result<Json<OkResponse>, (StatusCode, Json<ErrorResponse>)> {
    let user_id = user.0.id.clone();
    let settings = db::ensure_user(&state.db, &state.config, &user_id)
        .await
        .map_err(|_| json_invalid_argument())?;

    let req_bot_token = req
        .bot_token
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());
    let req_target = req
        .target
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());
    let saved_bot_token = settings
        .telegram_bot_token
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());
    let saved_target = settings
        .telegram_target
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty());

    let bot_token = req_bot_token.or(saved_bot_token).ok_or_else(|| {
        json_invalid_argument_with_message("缺少 bot token（可在本次请求提供或先在设置中保存）")
    })?;
    let target = req_target.or(saved_target).ok_or_else(|| {
        json_invalid_argument_with_message("缺少 target（可在本次请求提供或先在设置中保存）")
    })?;

    let text = req
        .text
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            format!(
                "catnap 测试消息\nuser={}\n{}",
                user_id,
                OffsetDateTime::now_utc()
                    .format(&Rfc3339)
                    .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
            )
        });

    match crate::notifications::send_telegram(
        &state.config.telegram_api_base_url,
        bot_token,
        target,
        &text,
    )
    .await
    {
        Ok(()) => {
            db::insert_log(
                &state.db,
                Some(&user_id),
                "info",
                "notify.telegram.test",
                "telegram test sent",
                None,
            )
            .await
            .map_err(|_| json_internal_error())?;
            Ok(Json(OkResponse { ok: true }))
        }
        Err(err) => {
            warn!(user_id, error = %err, "telegram test failed");
            let _ = db::insert_log(
                &state.db,
                Some(&user_id),
                "warn",
                "notify.telegram.test",
                "telegram test failed",
                Some(serde_json::json!({ "error": err.to_string() })),
            )
            .await;
            Err(json_internal_error_with_message(format!(
                "Telegram: {}",
                err
            )))
        }
    }
}

async fn post_web_push_test(
    State(state): State<AppState>,
    user: axum::extract::Extension<UserView>,
    Json(req): Json<WebPushTestRequest>,
) -> Result<Json<OkResponse>, (StatusCode, Json<ErrorResponse>)> {
    use web_push::{
        ContentEncoding, HyperWebPushClient, SubscriptionInfo, VapidSignatureBuilder,
        WebPushClient, WebPushMessageBuilder,
    };

    let user_id = user.0.id.clone();

    let subscription = db::get_latest_web_push_subscription(&state.db, &user_id)
        .await
        .map_err(|_| json_internal_error())?
        .ok_or_else(|| {
            json_invalid_argument_with_message(
                "缺少已保存的 Web Push subscription（请先“启用推送”并上传订阅）",
            )
        })?;

    validate_web_push_endpoint(
        subscription.endpoint.as_str(),
        state.config.allow_insecure_local_web_push_endpoints,
    )
    .await?;

    let Some(vapid_private_key) = state.config.web_push_vapid_private_key.as_deref() else {
        return Err(json_internal_error_with_message(
            "缺少 CATNAP_WEB_PUSH_VAPID_PRIVATE_KEY（服务端未配置，无法发送测试 Push）",
        ));
    };
    let Some(vapid_subject) = state.config.web_push_vapid_subject.as_deref() else {
        return Err(json_internal_error_with_message(
            "缺少 CATNAP_WEB_PUSH_VAPID_SUBJECT（服务端未配置，无法发送测试 Push）",
        ));
    };

    let endpoint = subscription.endpoint.trim();
    let p256dh = subscription.keys.p256dh.trim();
    let auth = subscription.keys.auth.trim();
    if endpoint.is_empty() || p256dh.is_empty() || auth.is_empty() {
        return Err(json_invalid_argument_with_message(
            "Web Push subscription 不完整（请重新上传订阅）",
        ));
    }

    let subscription_info = SubscriptionInfo::new(endpoint, p256dh, auth);

    let mut sig_builder = VapidSignatureBuilder::from_base64(vapid_private_key, &subscription_info)
        .map_err(|_| json_internal_error_with_message("Web Push: VAPID private key 无效"))?;
    sig_builder.add_claim("sub", vapid_subject);
    let signature = sig_builder
        .build()
        .map_err(|_| json_internal_error_with_message("Web Push: VAPID 签名生成失败"))?;

    let title = req
        .title
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or("catnap");
    let body = req.body.as_deref().unwrap_or("").to_string();
    let url = req
        .url
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or("/");

    let payload = serde_json::to_vec(&serde_json::json!({
        "title": title,
        "body": body,
        "url": url,
    }))
    .map_err(|_| json_internal_error())?;

    let mut builder = WebPushMessageBuilder::new(&subscription_info);
    builder.set_payload(ContentEncoding::Aes128Gcm, &payload);
    builder.set_ttl(60);
    builder.set_vapid_signature(signature);

    let message = builder
        .build()
        .map_err(|_| json_internal_error_with_message("Web Push: message build failed"))?;

    let client = HyperWebPushClient::new();

    match tokio::time::timeout(std::time::Duration::from_secs(10), client.send(message)).await {
        Ok(Ok(())) => {
            db::insert_log(
                &state.db,
                Some(&user_id),
                "info",
                "notify.web_push.test",
                "web push test sent",
                None,
            )
            .await
            .map_err(|_| json_internal_error())?;
            Ok(Json(OkResponse { ok: true }))
        }
        Ok(Err(err)) => {
            warn!(user_id, error = %err, "web push test failed");
            let _ = db::insert_log(
                &state.db,
                Some(&user_id),
                "warn",
                "notify.web_push.test",
                "web push test failed",
                Some(serde_json::json!({ "error": err.to_string(), "kind": err.short_description() })),
            )
            .await;
            Err(json_internal_error_with_message(format!(
                "Web Push: {}",
                err.short_description()
            )))
        }
        Err(_) => {
            warn!(user_id, "web push test timeout");
            let _ = db::insert_log(
                &state.db,
                Some(&user_id),
                "warn",
                "notify.web_push.test",
                "web push test timeout",
                None,
            )
            .await;
            Err(json_internal_error_with_message("Web Push: timeout"))
        }
    }
}

async fn validate_web_push_endpoint(
    endpoint: &str,
    allow_insecure_local: bool,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        return Err(json_invalid_argument_with_message(
            "subscription.endpoint 不能为空",
        ));
    }

    let uri: axum::http::Uri = endpoint
        .parse()
        .map_err(|_| json_invalid_argument_with_message("subscription.endpoint 不是合法 URL"))?;

    let scheme = uri
        .scheme_str()
        .ok_or_else(|| json_invalid_argument_with_message("subscription.endpoint 缺少 scheme"))?;

    if allow_insecure_local {
        if scheme != "http" && scheme != "https" {
            return Err(json_invalid_argument_with_message(
                "subscription.endpoint scheme 仅支持 http/https",
            ));
        }
        return Ok(());
    }

    if scheme != "https" {
        return Err(json_invalid_argument_with_message(
            "subscription.endpoint 必须为 https",
        ));
    }

    let authority = uri
        .authority()
        .ok_or_else(|| json_invalid_argument_with_message("subscription.endpoint 缺少 host"))?;
    let host = authority.host().trim().to_ascii_lowercase();

    if host == "localhost" || host.ends_with(".localhost") {
        return Err(json_invalid_argument_with_message(
            "subscription.endpoint host 不允许为 localhost",
        ));
    }

    if let Some(port) = authority.port_u16() {
        if port != 443 {
            return Err(json_invalid_argument_with_message(
                "subscription.endpoint 仅允许 443 端口",
            ));
        }
    }

    if let Ok(ip) = host.parse::<IpAddr>() {
        if !is_public_ip(ip) {
            return Err(json_invalid_argument_with_message(
                "subscription.endpoint 不允许指向私网/本机地址",
            ));
        }
        return Ok(());
    }

    let Ok(addrs) = tokio::net::lookup_host((host.as_str(), 443)).await else {
        return Err(json_invalid_argument_with_message(
            "subscription.endpoint host 无法解析",
        ));
    };

    if addrs.map(|a| a.ip()).any(|ip| !is_public_ip(ip)) {
        return Err(json_invalid_argument_with_message(
            "subscription.endpoint 不允许指向私网/本机地址",
        ));
    }

    Ok(())
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_multicast()
                || v4.is_unspecified()
                || v4.octets()[0] == 0)
        }
        IpAddr::V6(v6) => {
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || v6.is_unique_local()
                || v6.is_unicast_link_local())
        }
    }
}
