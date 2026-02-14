use axum::{
    body::{to_bytes, Body},
    http::{self, Request, StatusCode},
};
use catnap::{build_app, AppState, RuntimeConfig};
use futures_util::StreamExt;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tower::ServiceExt;

fn test_config() -> RuntimeConfig {
    RuntimeConfig {
        bind_addr: "127.0.0.1:0".to_string(),
        effective_version: "test".to_string(),
        repo_url: "https://example.com/repo".to_string(),
        github_api_base_url: "https://api.github.com".to_string(),
        update_check_repo: "IvanLi-CN/catnap".to_string(),
        update_check_enabled: false,
        update_check_ttl_seconds: 3600,
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
        allow_insecure_local_web_push_endpoints: true,
    }
}

struct TestApp {
    app: axum::Router,
    db: SqlitePool,
}

async fn make_app() -> TestApp {
    let cfg = test_config();
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&cfg.db_url)
        .await
        .unwrap();
    catnap::db::init_db(&db).await.unwrap();

    let snapshot = catnap::upstream::CatalogSnapshot::empty(cfg.upstream_cart_url.clone());
    let catalog = std::sync::Arc::new(tokio::sync::RwLock::new(snapshot));
    let ops = catnap::ops::OpsManager::new(cfg.clone(), db.clone(), catalog.clone());
    ops.start();

    let update_checker = catnap::updates::UpdateChecker::new(cfg.clone());

    let state = AppState {
        config: cfg,
        db: db.clone(),
        catalog,
        catalog_refresh: catnap::catalog_refresh::CatalogRefreshManager::new(),
        ops,
        update_checker,
    };

    TestApp {
        app: build_app(state),
        db,
    }
}

fn authed(req: http::request::Builder) -> Request<Body> {
    req.header("host", "example.com")
        .header("origin", "http://example.com")
        .header("x-user", "u_1")
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn ops_state_snapshot_works() {
    let t = make_app().await;
    let res = t
        .app
        .oneshot(authed(Request::builder().uri("/api/ops/state")))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(json.get("serverTime").is_some());
    assert!(json.get("queue").is_some());
    assert!(json.get("workers").is_some());
    assert!(json.get("stats").is_some());
    assert!(json.get("logTail").is_some());
}

#[tokio::test]
async fn ops_state_rejects_invalid_params() {
    let t = make_app().await;
    let res = t
        .app
        .oneshot(authed(
            Request::builder().uri("/api/ops/state?range=wat&logLimit=999&taskLimit=0"),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn ops_stream_is_sse_and_can_reset_on_invalid_last_event_id() {
    let t = make_app().await;
    let res = t
        .app
        .oneshot(authed(
            Request::builder()
                .uri("/api/ops/stream?range=24h")
                .header("last-event-id", "nope"),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let content_type = res
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(content_type.contains("text/event-stream"));

    let mut stream = res.into_body().into_data_stream();
    let mut buf = String::new();
    for _ in 0..4 {
        if let Some(chunk) = stream.next().await {
            let chunk = chunk.unwrap();
            buf.push_str(&String::from_utf8_lossy(&chunk));
            if buf.contains("event: ops.reset") {
                break;
            }
        }
    }
    assert!(buf.contains("event: ops.hello"));
    assert!(buf.contains("event: ops.reset"));
}

#[tokio::test]
async fn ops_stream_replays_events_by_last_event_id() {
    let t = make_app().await;
    let ts = OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());

    let msg1 = serde_json::json!({
        "ts": ts,
        "level": "info",
        "scope": "test",
        "message": "hello-1",
        "meta": null,
    })
    .to_string();

    let res = sqlx::query("INSERT INTO ops_events (ts, event, data_json) VALUES (?, 'ops.log', ?)")
        .bind(&ts)
        .bind(&msg1)
        .execute(&t.db)
        .await
        .unwrap();
    let id1 = res.last_insert_rowid();

    let msg2 = serde_json::json!({
        "ts": ts,
        "level": "info",
        "scope": "test",
        "message": "hello-2",
        "meta": null,
    })
    .to_string();
    let res = sqlx::query("INSERT INTO ops_events (ts, event, data_json) VALUES (?, 'ops.log', ?)")
        .bind(&ts)
        .bind(&msg2)
        .execute(&t.db)
        .await
        .unwrap();
    let _id2 = res.last_insert_rowid();

    let res = t
        .app
        .oneshot(authed(
            Request::builder()
                .uri("/api/ops/stream?range=24h")
                .header("last-event-id", id1.to_string()),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let mut stream = res.into_body().into_data_stream();
    let mut buf = String::new();
    for _ in 0..6 {
        if let Some(chunk) = stream.next().await {
            let chunk = chunk.unwrap();
            buf.push_str(&String::from_utf8_lossy(&chunk));
            if (buf.contains("event: ops.log") || buf.contains("event:ops.log"))
                && buf.contains("hello-2")
            {
                break;
            }
        }
    }
    assert!(buf.contains("event: ops.hello"));
    assert!(buf.contains("ops.log"));
    assert!(buf.contains("hello-2"));
}
