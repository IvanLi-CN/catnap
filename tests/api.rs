use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use catnap::{build_app, AppState, RuntimeConfig};
use sqlx::sqlite::SqlitePoolOptions;
use std::{collections::HashMap, path::PathBuf, sync::Arc};
use time::OffsetDateTime;
use tower::ServiceExt;

fn test_config() -> RuntimeConfig {
    RuntimeConfig {
        bind_addr: "127.0.0.1:0".to_string(),
        static_dir: "web/dist".to_string(),
        effective_version: "test".to_string(),
        upstream_cart_url: "https://lazycats.online/cart".to_string(),
        auth_user_header: Some("x-user".to_string()),
        default_poll_interval_minutes: 1,
        default_poll_jitter_pct: 0.1,
        log_retention_days: 7,
        log_retention_max_rows: 10_000,
        db_url: "sqlite::memory:".to_string(),
        web_push_vapid_public_key: None,
    }
}

async fn make_app() -> axum::Router {
    let cfg = test_config();
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

    let state = AppState {
        config: cfg,
        db: db.clone(),
        catalog: std::sync::Arc::new(tokio::sync::RwLock::new(snapshot)),
        manual_refresh_gate: Arc::new(tokio::sync::Mutex::new(
            HashMap::<String, OffsetDateTime>::new(),
        )),
        manual_refresh_status: Arc::new(tokio::sync::Mutex::new(HashMap::<
            String,
            catnap::models::RefreshStatusResponse,
        >::new())),
    };

    build_app(state, PathBuf::from("web/dist"))
}

#[tokio::test]
async fn api_requires_user_header() {
    let app = make_app().await;
    let res = app
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
    let app = make_app().await;
    let res = app
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
    let app = make_app().await;
    let res = app
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
    let app = make_app().await;
    let res = app
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
async fn monitoring_toggle_persists() {
    let app = make_app().await;
    let bytes = serde_json::to_vec(&serde_json::json!({ "enabled": true })).unwrap();

    // Toggle a monitor-supported config (fid=7).
    let toggle_res = app
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

    let bootstrap_res = app
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
