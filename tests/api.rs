use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use catnap::{build_app, AppState, RuntimeConfig};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::Row;
use sqlx::SqlitePool;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use time::OffsetDateTime;
use tower::ServiceExt;

fn test_config() -> RuntimeConfig {
    RuntimeConfig {
        bind_addr: "127.0.0.1:0".to_string(),
        effective_version: "test".to_string(),
        upstream_cart_url: "https://lazycats.vip/cart".to_string(),
        telegram_api_base_url: "https://api.telegram.org".to_string(),
        auth_user_header: Some("x-user".to_string()),
        dev_user_id: None,
        default_poll_interval_minutes: 1,
        default_poll_jitter_pct: 0.1,
        log_retention_days: 7,
        log_retention_max_rows: 10_000,
        ops_worker_concurrency: 1,
        ops_sse_replay_window_seconds: 3600,
        ops_log_retention_days: 7,
        ops_log_tail_limit_default: 200,
        ops_queue_task_limit_default: 200,
        db_url: "sqlite::memory:".to_string(),
        web_push_vapid_public_key: None,
        web_push_vapid_private_key: None,
        web_push_vapid_subject: None,
        allow_insecure_local_web_push_endpoints: false,
    }
}

struct TestApp {
    app: axum::Router,
    db: SqlitePool,
}

async fn make_app() -> TestApp {
    make_app_with_config(test_config()).await
}

async fn make_app_with_config(cfg: RuntimeConfig) -> TestApp {
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&cfg.db_url)
        .await
        .unwrap();
    catnap::db::init_db(&db).await.unwrap();

    let countries = catnap::upstream::parse_countries(include_str!("fixtures/cart-root.html"));
    let regions = catnap::upstream::parse_regions("2", include_str!("fixtures/cart-fid-2.html"));
    let mut configs = Vec::new();
    configs.extend(catnap::upstream::parse_configs(
        "2",
        Some("56"),
        include_str!("fixtures/cart-fid-2-gid-56.html"),
    ));
    configs.extend(catnap::upstream::parse_configs(
        "7",
        Some("40"),
        include_str!("fixtures/cart-fid-7.html"),
    ));

    let snapshot = catnap::upstream::CatalogSnapshot {
        countries,
        regions,
        configs: configs.clone(),
        fetched_at: "2026-01-19T00:00:00Z".to_string(),
        source_url: cfg.upstream_cart_url.clone(),
    };

    catnap::db::upsert_catalog_configs(&db, &configs)
        .await
        .unwrap();

    let catalog = std::sync::Arc::new(tokio::sync::RwLock::new(snapshot));
    let ops = catnap::ops::OpsManager::new(cfg.clone(), db.clone(), catalog.clone());
    ops.start();

    let state = AppState {
        config: cfg,
        db: db.clone(),
        catalog,
        catalog_refresh: catnap::catalog_refresh::CatalogRefreshManager::new(),
        ops,
    };

    TestApp {
        app: build_app(state),
        db,
    }
}

async fn spawn_stub_server(app: axum::Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{}", addr)
}

#[tokio::test]
async fn api_requires_user_header() {
    let t = make_app().await;
    let res = t
        .app
        .oneshot(
            Request::builder()
                .uri("/api/bootstrap")
                .header("host", "example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn ui_requires_user_header_with_html_401() {
    let t = make_app().await;
    let res = t
        .app
        .oneshot(
            Request::builder()
                .uri("/")
                .header("host", "example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let content_type = res
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(content_type.contains("text/html"));
}

#[tokio::test]
async fn same_origin_is_enforced_for_api_requests() {
    let t = make_app().await;
    let res = t
        .app
        .oneshot(
            Request::builder()
                .uri("/api/bootstrap")
                .header("host", "example.com")
                .header("x-user", "u_1")
                .header("origin", "http://evil.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn bootstrap_returns_catalog_and_settings() {
    let t = make_app().await;
    let res = t
        .app
        .oneshot(
            Request::builder()
                .uri("/api/bootstrap")
                .header("host", "example.com")
                .header("x-user", "u_1")
                .header("origin", "http://example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(json.get("catalog").is_some());
    assert!(json.get("settings").is_some());
    assert!(json.get("monitoring").is_some());
}

#[tokio::test]
async fn same_origin_accepts_last_forwarded_values() {
    let t = make_app().await;
    let res = t
        .app
        .oneshot(
            Request::builder()
                .uri("/api/bootstrap")
                .header("host", "example.com")
                .header("x-user", "u_1")
                .header("origin", "https://example.com")
                .header("x-forwarded-host", "evil.com, example.com")
                .header("x-forwarded-proto", "http, https")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn same_origin_rejects_first_forwarded_values() {
    let t = make_app().await;
    let res = t
        .app
        .oneshot(
            Request::builder()
                .uri("/api/bootstrap")
                .header("host", "example.com")
                .header("x-user", "u_1")
                .header("origin", "https://evil.com")
                .header("x-forwarded-host", "evil.com, example.com")
                .header("x-forwarded-proto", "https, https")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn monitoring_toggle_persists() {
    let t = make_app().await;
    let bytes = serde_json::to_vec(&serde_json::json!({ "enabled": true })).unwrap();

    // Toggle a monitor-supported config (fid=7).
    let toggle_res = t
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/monitoring/configs/lc:7:40:128")
                .header("host", "example.com")
                .header("x-user", "u_1")
                .header("origin", "http://example.com")
                .header("content-type", "application/json")
                .body(Body::from(bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(toggle_res.status(), StatusCode::OK);

    let bootstrap_res = t
        .app
        .oneshot(
            Request::builder()
                .uri("/api/bootstrap")
                .header("host", "example.com")
                .header("x-user", "u_1")
                .header("origin", "http://example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bootstrap_res.status(), StatusCode::OK);

    let bytes = to_bytes(bootstrap_res.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let enabled = json["monitoring"]["enabledConfigIds"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v.as_str() == Some("lc:7:40:128"));
    assert!(enabled);
}

#[tokio::test]
async fn telegram_test_returns_400_when_missing_token_or_target() {
    let t = make_app().await;
    let bytes = serde_json::to_vec(&serde_json::json!({
        "botToken": null,
        "target": null,
        "text": null,
    }))
    .unwrap();

    let res = t
        .app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/notifications/telegram/test")
                .header("host", "example.com")
                .header("x-user", "u_1")
                .header("origin", "http://example.com")
                .header("content-type", "application/json")
                .body(Body::from(bytes))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["error"]["code"], "INVALID_ARGUMENT");
}

#[tokio::test]
async fn telegram_test_returns_5xx_when_upstream_fails() {
    let tg = axum::Router::new().route(
        "/*path",
        axum::routing::post(|| async { StatusCode::UNAUTHORIZED }),
    );
    let base = spawn_stub_server(tg).await;

    let mut cfg = test_config();
    cfg.telegram_api_base_url = base;
    let t = make_app_with_config(cfg).await;

    // Ensure user + default settings row exists.
    let res = t
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/bootstrap")
                .header("host", "example.com")
                .header("x-user", "u_1")
                .header("origin", "http://example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // Save settings so request body can omit botToken/target.
    let bytes = serde_json::to_vec(&serde_json::json!({
        "poll": { "intervalMinutes": 1, "jitterPct": 0.1 },
        "siteBaseUrl": null,
        "notifications": {
            "telegram": { "enabled": true, "botToken": "t", "target": "@c" },
            "webPush": { "enabled": false }
        }
    }))
    .unwrap();
    let res = t
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/settings")
                .header("host", "example.com")
                .header("x-user", "u_1")
                .header("origin", "http://example.com")
                .header("content-type", "application/json")
                .body(Body::from(bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let bytes =
        serde_json::to_vec(&serde_json::json!({ "botToken": null, "target": null, "text": "hi" }))
            .unwrap();
    let res = t
        .app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/notifications/telegram/test")
                .header("host", "example.com")
                .header("x-user", "u_1")
                .header("origin", "http://example.com")
                .header("content-type", "application/json")
                .body(Body::from(bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn web_push_test_hits_subscription_endpoint() {
    let hits = Arc::new(AtomicUsize::new(0));
    let hits2 = hits.clone();
    let push = axum::Router::new().route(
        "/*path",
        axum::routing::post(move || {
            let hits = hits2.clone();
            async move {
                hits.fetch_add(1, Ordering::SeqCst);
                StatusCode::CREATED
            }
        }),
    );
    let push_base = spawn_stub_server(push).await;

    let mut cfg = test_config();
    cfg.web_push_vapid_private_key =
        Some("IQ9Ur0ykXoHS9gzfYX0aBjy9lvdrjx_PFUXmie9YRcY".to_string());
    cfg.web_push_vapid_subject = Some("mailto:test@example.com".to_string());
    cfg.allow_insecure_local_web_push_endpoints = true;
    let t = make_app_with_config(cfg).await;

    sqlx::query(
        r#"INSERT INTO web_push_subscriptions (id, user_id, endpoint, p256dh, auth, created_at)
           VALUES (?, ?, ?, ?, ?, ?)"#,
    )
    .bind("sub_1")
    .bind("u_1")
    .bind(format!("{}/push", push_base))
    .bind("BLMbF9ffKBiWQLCKvTHb6LO8Nb6dcUh6TItC455vu2kElga6PQvUmaFyCdykxY2nOSSL3yKgfbmFLRTUaGv4yV8")
    .bind("xS03Fi5ErfTNH_l9WHE9Ig")
    .bind("2026-01-24T00:00:00Z")
    .execute(&t.db)
    .await
    .unwrap();

    let bytes = serde_json::to_vec(&serde_json::json!({
        "title": "catnap",
        "body": "test",
        "url": "/settings"
    }))
    .unwrap();

    let res = t
        .app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/notifications/web-push/test")
                .header("host", "example.com")
                .header("x-user", "u_1")
                .header("origin", "http://example.com")
                .header("content-type", "application/json")
                .body(Body::from(bytes))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn logs_cursor_paginates_with_rfc3339_timestamps() {
    let t = make_app().await;

    // Use a fixed RFC3339 timestamp containing ":" to ensure the cursor parser round-trips.
    // If the cursor is split from the left, pagination will break.
    let ts = "2026-01-19T00:00:00Z";
    sqlx::query(
        "INSERT INTO event_logs (id, user_id, ts, level, scope, message, meta_json) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("b")
    .bind("u_1")
    .bind(ts)
    .bind("info")
    .bind("test")
    .bind("second")
    .bind(Option::<String>::None)
    .execute(&t.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO event_logs (id, user_id, ts, level, scope, message, meta_json) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("a")
    .bind("u_1")
    .bind(ts)
    .bind("info")
    .bind("test")
    .bind("first")
    .bind(Option::<String>::None)
    .execute(&t.db)
    .await
    .unwrap();

    // First page.
    let res = t
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/logs?limit=1")
                .header("host", "example.com")
                .header("x-user", "u_1")
                .header("origin", "http://example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let items = json["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"].as_str(), Some("b"));
    let cursor = json["nextCursor"].as_str().unwrap().to_string();

    // Second page (should contain the remaining row).
    let res = t
        .app
        .oneshot(
            Request::builder()
                .uri(format!("/api/logs?limit=1&cursor={cursor}"))
                .header("host", "example.com")
                .header("x-user", "u_1")
                .header("origin", "http://example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let items = json["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"].as_str(), Some("a"));
}

#[tokio::test]
async fn inventory_history_batch_query_returns_sparse_points() {
    let t = make_app().await;

    let ids = sqlx::query("SELECT id FROM catalog_configs ORDER BY id LIMIT 2")
        .fetch_all(&t.db)
        .await
        .unwrap()
        .into_iter()
        .map(|r| r.get::<String, _>(0))
        .collect::<Vec<_>>();
    assert!(ids.len() >= 2);
    let id1 = ids[0].clone();
    let id2 = ids[1].clone();

    let now = OffsetDateTime::now_utc()
        .replace_second(0)
        .unwrap()
        .replace_nanosecond(0)
        .unwrap();
    let ts1 = now - time::Duration::minutes(3);
    let ts2 = now - time::Duration::minutes(1);
    let ts1 = ts1
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let ts2 = ts2
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();

    // make_app() may have inserted a sample via upsert_catalog_configs; keep this test deterministic.
    sqlx::query("DELETE FROM inventory_samples_1m WHERE config_id = ?")
        .bind(&id1)
        .execute(&t.db)
        .await
        .unwrap();
    sqlx::query("DELETE FROM inventory_samples_1m WHERE config_id = ?")
        .bind(&id2)
        .execute(&t.db)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO inventory_samples_1m (config_id, ts_minute, inventory_quantity) VALUES (?, ?, ?)",
    )
    .bind(&id1)
    .bind(&ts1)
    .bind(0_i64)
    .execute(&t.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO inventory_samples_1m (config_id, ts_minute, inventory_quantity) VALUES (?, ?, ?)",
    )
    .bind(&id1)
    .bind(&ts2)
    .bind(12_i64)
    .execute(&t.db)
    .await
    .unwrap();

    let body = serde_json::to_vec(&serde_json::json!({ "configIds": [id1, id2] })).unwrap();
    let res = t
        .app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/inventory/history")
                .header("host", "example.com")
                .header("x-user", "u_1")
                .header("origin", "http://example.com")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let series = json["series"].as_array().unwrap();
    assert_eq!(series.len(), 2);
    assert_eq!(series[0]["configId"].as_str(), Some(ids[0].as_str()));
    assert_eq!(series[1]["configId"].as_str(), Some(ids[1].as_str()));

    let points = series[0]["points"].as_array().unwrap();
    assert_eq!(points.len(), 2);
    assert_eq!(points[0]["tsMinute"].as_str(), Some(ts1.as_str()));
    assert_eq!(points[0]["quantity"].as_i64(), Some(0));
    assert_eq!(points[1]["tsMinute"].as_str(), Some(ts2.as_str()));
    assert_eq!(points[1]["quantity"].as_i64(), Some(12));

    let points2 = series[1]["points"].as_array().unwrap();
    assert!(points2.is_empty());
}

#[tokio::test]
async fn inventory_history_rejects_empty_config_ids() {
    let t = make_app().await;
    let body = serde_json::to_vec(&serde_json::json!({ "configIds": [] })).unwrap();
    let res = t
        .app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/inventory/history")
                .header("host", "example.com")
                .header("x-user", "u_1")
                .header("origin", "http://example.com")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}
