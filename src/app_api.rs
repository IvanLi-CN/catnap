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
use std::net::IpAddr;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tracing::warn;

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
