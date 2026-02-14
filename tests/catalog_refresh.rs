use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use catnap::{build_app, AppState, RuntimeConfig};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::Row;
use sqlx::SqlitePool;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tower::ServiceExt;

fn base_test_config() -> RuntimeConfig {
    RuntimeConfig {
        bind_addr: "127.0.0.1:0".to_string(),
        effective_version: "test".to_string(),
        repo_url: "https://example.com/repo".to_string(),
        github_api_base_url: "https://api.github.com".to_string(),
        update_check_repo: "IvanLi-CN/catnap".to_string(),
        update_check_enabled: false,
        update_check_ttl_seconds: 3600,
        upstream_cart_url: "http://127.0.0.1:0/cart".to_string(),
        telegram_api_base_url: "https://api.telegram.org".to_string(),
        auth_user_header: Some("x-user".to_string()),
        dev_user_id: None,
        default_poll_interval_minutes: 1,
        default_poll_jitter_pct: 0.1,
        log_retention_days: 7,
        log_retention_max_rows: 10_000,
        ops_worker_concurrency: 2,
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

async fn make_app_with_config(cfg: RuntimeConfig) -> TestApp {
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

async fn spawn_stub_server(app: axum::Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{}", addr)
}

#[tokio::test]
async fn catalog_refresh_events_endpoint_is_sse() {
    let t = make_app_with_config(base_test_config()).await;
    let res = t
        .app
        .oneshot(
            Request::builder()
                .uri("/api/catalog/refresh/events")
                .header("host", "example.com")
                .header("x-user", "u_1")
                .header("origin", "http://example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let content_type = res
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(content_type.contains("text/event-stream"));
}

#[tokio::test]
async fn catalog_refresh_job_runs_and_persists_url_cache() {
    let cart_root_only_fid2 = r#"
<!doctype html>
<div class="firstgroup_item" onclick="window.location.href='/cart?fid=2'">
  <span class="yy-bth-text-a">CN</span>
</div>
"#;

    let cart_fid_2 = include_str!("fixtures/cart-fid-2.html");
    let cart_fid_2_gid_56 = include_str!("fixtures/cart-fid-2-gid-56.html");

    #[derive(serde::Deserialize)]
    struct CartQuery {
        fid: Option<String>,
        gid: Option<String>,
    }

    let hits = Arc::new(AtomicUsize::new(0));
    let hits_for_handler = hits.clone();
    let upstream = axum::Router::new().route(
        "/cart",
        axum::routing::get(
            move |axum::extract::Query(q): axum::extract::Query<CartQuery>| async move {
                hits_for_handler.fetch_add(1, Ordering::SeqCst);
                match (q.fid.as_deref(), q.gid.as_deref()) {
                    (None, None) => (StatusCode::OK, cart_root_only_fid2),
                    (Some("2"), None) => (StatusCode::OK, cart_fid_2),
                    (Some("2"), Some(_)) => (StatusCode::OK, cart_fid_2_gid_56),
                    _ => (StatusCode::NOT_FOUND, "not found"),
                }
            },
        ),
    );
    let base = spawn_stub_server(upstream).await;

    let mut cfg = base_test_config();
    cfg.upstream_cart_url = format!("{base}/cart");

    async fn post_refresh(app: axum::Router, user: &str) -> StatusCode {
        app.oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/catalog/refresh")
                .header("host", "example.com")
                .header("origin", "http://example.com")
                .header("x-user", user)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
    }

    async fn wait_refresh_done(app: axum::Router) -> String {
        let mut state = "idle".to_string();
        for _ in 0..80 {
            let res = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/api/refresh/status")
                        .header("host", "example.com")
                        .header("x-user", "u_1")
                        .header("origin", "http://example.com")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::OK);
            let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
                .await
                .unwrap();
            let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            state = json["state"].as_str().unwrap_or("").to_string();
            if state == "success" || state == "error" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        state
    }

    // Baseline: single trigger.
    hits.store(0, Ordering::SeqCst);
    let t1 = make_app_with_config(cfg.clone()).await;
    assert_eq!(post_refresh(t1.app.clone(), "u_1").await, StatusCode::OK);
    assert_eq!(wait_refresh_done(t1.app.clone()).await, "success");
    let baseline_hits = hits.load(Ordering::SeqCst);

    // Concurrency: two near-simultaneous triggers must not start two jobs.
    hits.store(0, Ordering::SeqCst);
    let t2 = make_app_with_config(cfg).await;
    let (s1, s2) = tokio::join!(
        post_refresh(t2.app.clone(), "u_1"),
        post_refresh(t2.app.clone(), "u_2"),
    );
    assert_eq!(s1, StatusCode::OK);
    assert_eq!(s2, StatusCode::OK);
    assert_eq!(wait_refresh_done(t2.app.clone()).await, "success");

    // URL cache persists last-good results.
    let rows = sqlx::query("SELECT url_key FROM catalog_url_cache")
        .fetch_all(&t2.db)
        .await
        .unwrap();
    let keys = rows
        .into_iter()
        .map(|r| r.get::<String, _>(0))
        .collect::<Vec<_>>();
    assert!(keys.contains(&"2:56".to_string()));

    // Fetched configs are marked as active.
    let row = sqlx::query("SELECT lifecycle_state FROM catalog_configs LIMIT 1")
        .fetch_one(&t2.db)
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>(0), "active");

    assert_eq!(hits.load(Ordering::SeqCst), baseline_hits);
}

#[tokio::test]
async fn lifecycle_marks_delisted_and_relisted() {
    let cfg = base_test_config();
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&cfg.db_url)
        .await
        .unwrap();
    catnap::db::init_db(&db).await.unwrap();

    let mut configs =
        catnap::upstream::parse_configs("7", Some("40"), include_str!("fixtures/cart-fid-7.html"));
    assert!(configs.len() >= 2);

    // Baseline: only keep two configs and seed both as active.
    configs.truncate(2);
    catnap::db::upsert_catalog_configs(&db, &configs)
        .await
        .unwrap();

    // Apply a success that misses the second config => delist.
    let only_first = vec![configs[0].clone()];
    let res = catnap::db::apply_catalog_url_fetch_success(
        &db,
        "7",
        Some("40"),
        "7:40",
        "http://example.invalid/cart?fid=7&gid=40",
        only_first,
    )
    .await
    .unwrap();
    assert!(res.delisted_ids.contains(&configs[1].id));

    let row = sqlx::query(
        "SELECT lifecycle_state, lifecycle_delisted_at FROM catalog_configs WHERE id = ?",
    )
    .bind(&configs[1].id)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(row.get::<String, _>(0), "delisted");
    assert!(row.get::<Option<String>, _>(1).is_some());

    // Apply a success that includes it again => relist (active + clear delisted_at).
    let both = configs.clone();
    let res2 = catnap::db::apply_catalog_url_fetch_success(
        &db,
        "7",
        Some("40"),
        "7:40",
        "http://example.invalid/cart?fid=7&gid=40",
        both,
    )
    .await
    .unwrap();
    assert!(res2.listed_ids.contains(&configs[1].id));

    let row2 = sqlx::query(
        "SELECT lifecycle_state, lifecycle_delisted_at FROM catalog_configs WHERE id = ?",
    )
    .bind(&configs[1].id)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(row2.get::<String, _>(0), "active");
    assert!(row2.get::<Option<String>, _>(1).is_none());
}
