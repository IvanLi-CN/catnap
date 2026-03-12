use axum::{http::StatusCode, Router};
use catnap::{AppState, RuntimeConfig};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Row, SqlitePool};
use std::sync::Arc;
use tokio::sync::RwLock;

fn test_config() -> RuntimeConfig {
    RuntimeConfig {
        bind_addr: "127.0.0.1:0".to_string(),
        effective_version: "test".to_string(),
        repo_url: "https://example.com/repo".to_string(),
        update_repo: "example/repo".to_string(),
        update_check_enabled: false,
        update_check_ttl_seconds: 0,
        update_check_timeout_ms: 1500,
        github_api_base_url: "https://api.github.com".to_string(),
        upstream_cart_url: "https://example.invalid/cart".to_string(),
        telegram_api_base_url: "https://api.telegram.org".to_string(),
        auth_user_header: Some("x-user".to_string()),
        dev_user_id: None,
        default_poll_interval_minutes: 1,
        default_poll_jitter_pct: 0.1,
        log_retention_days: 7,
        log_retention_max_rows: 10_000,
        notification_retention_days: 30,
        notification_retention_max_rows: 50_000,
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

async fn spawn_stub_server(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{}", addr)
}

async fn build_state(cfg: RuntimeConfig, db: SqlitePool) -> AppState {
    let catalog = Arc::new(RwLock::new(
        catnap::db::load_catalog_snapshot(&db, &cfg.upstream_cart_url)
            .await
            .unwrap(),
    ));
    let ops = catnap::ops::OpsManager::new(cfg.clone(), db.clone(), catalog.clone());
    AppState {
        config: cfg,
        db,
        catalog,
        catalog_refresh: catnap::catalog_refresh::CatalogRefreshManager::default(),
        ops,
        update_cache: Arc::new(RwLock::new(
            catnap::update_check::UpdateCheckCache::default(),
        )),
    }
}

#[tokio::test]
async fn refresh_catalog_topology_rejects_empty_parse_when_state_exists() {
    let upstream = Router::new().route(
        "/cart",
        axum::routing::get(|| async { "<!doctype html><html><body>maintenance</body></html>" }),
    );
    let base = spawn_stub_server(upstream).await;

    let mut cfg = test_config();
    cfg.upstream_cart_url = format!("{base}/cart");

    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&cfg.db_url)
        .await
        .unwrap();
    catnap::db::init_db(&db).await.unwrap();

    let countries = vec![catnap::models::Country {
        id: "2".to_string(),
        name: "CN".to_string(),
    }];
    catnap::db::replace_catalog_topology(&db, &cfg.upstream_cart_url, &countries, &[])
        .await
        .unwrap();

    let state = build_state(cfg.clone(), db.clone()).await;

    let err = catnap::poller::refresh_catalog_topology(&state, "test")
        .await
        .unwrap_err();
    assert!(err
        .to_string()
        .contains("refusing empty topology refresh while catalog state already exists"));

    let row = sqlx::query("SELECT COUNT(*) FROM catalog_countries")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(row.get::<i64, _>(0), 1);
}

#[tokio::test]
async fn refresh_catalog_topology_retires_removed_targets() {
    let root_html = r#"
<!doctype html>
<div class="firstgroup_item" onclick="window.location.href='/cart?fid=7'">
  <span class="yy-bth-text-a">JP</span>
</div>
"#;
    let fid_html = include_str!("fixtures/cart-fid-7.html");

    #[derive(serde::Deserialize)]
    struct CartQuery {
        fid: Option<String>,
    }

    let upstream = Router::new().route(
        "/cart",
        axum::routing::get(
            move |axum::extract::Query(q): axum::extract::Query<CartQuery>| async move {
                match q.fid.as_deref() {
                    None => root_html,
                    Some("7") => fid_html,
                    Some(_) => "not found",
                }
            },
        ),
    );
    let base = spawn_stub_server(upstream).await;

    let mut cfg = test_config();
    cfg.upstream_cart_url = format!("{base}/cart");

    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&cfg.db_url)
        .await
        .unwrap();
    catnap::db::init_db(&db).await.unwrap();

    let countries = vec![catnap::models::Country {
        id: "7".to_string(),
        name: "JP".to_string(),
    }];
    let regions = vec![catnap::models::Region {
        id: "40".to_string(),
        country_id: "7".to_string(),
        name: "Tokyo".to_string(),
        location_name: None,
    }];
    catnap::db::replace_catalog_topology(&db, &cfg.upstream_cart_url, &countries, &regions)
        .await
        .unwrap();

    let mut configs =
        catnap::upstream::parse_configs("7", Some("40"), include_str!("fixtures/cart-fid-7.html"));
    configs.truncate(1);
    catnap::db::upsert_catalog_configs(&db, &configs)
        .await
        .unwrap();

    for (user_id, region_enabled, site_enabled) in [
        ("u_region_scope", true, false),
        ("u_site_scope", false, true),
    ] {
        catnap::db::ensure_user(&db, &cfg, user_id).await.unwrap();
        sqlx::query(
            r#"
UPDATE settings
SET monitoring_events_partition_catalog_change_enabled = 0,
    monitoring_events_region_partition_change_enabled = ?,
    monitoring_events_site_region_change_enabled = ?,
    telegram_enabled = 0,
    web_push_enabled = 0
WHERE user_id = ?
"#,
        )
        .bind(if region_enabled { 1 } else { 0 })
        .bind(if site_enabled { 1 } else { 0 })
        .bind(user_id)
        .execute(&db)
        .await
        .unwrap();
    }
    catnap::db::set_monitoring_partition_enabled(&db, "u_region_scope", "7", None, true)
        .await
        .unwrap();
    catnap::db::replace_catalog_topology(
        &db,
        &cfg.upstream_cart_url,
        &[
            catnap::models::Country {
                id: "7".to_string(),
                name: "JP".to_string(),
            },
            catnap::models::Country {
                id: "8".to_string(),
                name: "FI".to_string(),
            },
        ],
        &regions,
    )
    .await
    .unwrap();

    let state = build_state(cfg.clone(), db.clone()).await;
    catnap::poller::refresh_catalog_topology(&state, "test")
        .await
        .unwrap();

    let row = sqlx::query("SELECT lifecycle_state FROM catalog_configs WHERE id = ?")
        .bind(&configs[0].id)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>(0), "delisted");

    let known_targets = catnap::db::list_known_catalog_targets(&db).await.unwrap();
    assert!(known_targets.is_empty());

    let rows = sqlx::query(
        "SELECT user_id, scope FROM event_logs WHERE scope LIKE 'catalog.%' ORDER BY user_id ASC, scope ASC",
    )
    .fetch_all(&db)
    .await
    .unwrap();
    let actual = rows
        .into_iter()
        .map(|row| (row.get::<String, _>(0), row.get::<String, _>(1)))
        .collect::<Vec<_>>();
    assert_eq!(
        actual,
        vec![
            (
                "u_region_scope".to_string(),
                "catalog.partition.removed".to_string(),
            ),
            (
                "u_site_scope".to_string(),
                "catalog.region.removed".to_string()
            ),
        ]
    );
}

#[tokio::test]
async fn probe_catalog_topology_discovers_new_targets_without_retiring_existing_ones() {
    let root_html = r#"
<!doctype html>
<div class="firstgroup_item" onclick="window.location.href='/cart?fid=2'">
  <span class="yy-bth-text-a">CN</span>
</div>
"#;
    let fid_html = format!(
        r#"{}
<div class="card cartitem shadow w-100">
  <div class="card-body">
    <h4>CN Direct Premium</h4>
    <div class="card-text mb-4 mt-3">
      <p>CPU：4核心</p>
      <p>内存：8G</p>
    </div>
  </div>
  <div class="ml-4">
    <p class="card-text">库存： 3</p>
  </div>
  <div class="text-right">
    ¥ <a class="cart-num DINCondensed-Bold">66.00</a> 元 / 月
  </div>
  <div class="card-footer">
    <a href="/cart?action=configureproduct&pid=256">立即购买</a>
  </div>
</div>
"#,
        include_str!("fixtures/cart-fid-2.html")
    );
    let gid_57_html = r#"
<!doctype html>
<html lang="zh-CN">
  <body>
    <div class="card cartitem shadow w-100">
      <div class="card-body">
        <h4>HKG Premium Plus</h4>
        <div class="card-text mb-4 mt-3">
          <p>CPU：2核心</p>
          <p>内存：2G</p>
        </div>
      </div>
      <div class="ml-4">
        <p class="card-text">库存： 5</p>
      </div>
      <div class="text-right">
        ¥ <a class="cart-num DINCondensed-Bold">88.00</a> 元 / 月
      </div>
      <div class="card-footer">
        <a href="/cart?action=configureproduct&pid=257">立即购买</a>
      </div>
    </div>
  </body>
</html>
"#;

    #[derive(serde::Deserialize)]
    struct CartQuery {
        fid: Option<String>,
        gid: Option<String>,
    }

    let upstream = Router::new().route(
        "/cart",
        axum::routing::get(
            move |axum::extract::Query(q): axum::extract::Query<CartQuery>| async move {
                match (q.fid.as_deref(), q.gid.as_deref()) {
                    (None, None) => root_html.to_string(),
                    (Some("2"), None) => fid_html.clone(),
                    (Some("2"), Some("57")) => gid_57_html.to_string(),
                    _ => "not found".to_string(),
                }
            },
        ),
    );
    let base = spawn_stub_server(upstream).await;

    let mut cfg = test_config();
    cfg.upstream_cart_url = format!("{base}/cart");

    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&cfg.db_url)
        .await
        .unwrap();
    catnap::db::init_db(&db).await.unwrap();

    let countries = vec![catnap::models::Country {
        id: "2".to_string(),
        name: "CN".to_string(),
    }];
    let regions = vec![catnap::models::Region {
        id: "56".to_string(),
        country_id: "2".to_string(),
        name: "HKG Premium".to_string(),
        location_name: Some("湾仔".to_string()),
    }];
    catnap::db::replace_catalog_topology(&db, &cfg.upstream_cart_url, &countries, &regions)
        .await
        .unwrap();

    let mut configs = catnap::upstream::parse_configs(
        "2",
        Some("56"),
        include_str!("fixtures/cart-fid-2-gid-56.html"),
    );
    configs.truncate(1);
    catnap::db::upsert_catalog_configs(&db, &configs)
        .await
        .unwrap();

    catnap::db::ensure_user(&db, &cfg, "u_region_scope")
        .await
        .unwrap();
    sqlx::query(
        r#"
UPDATE settings
SET monitoring_events_partition_catalog_change_enabled = 0,
    monitoring_events_region_partition_change_enabled = 1,
    monitoring_events_site_region_change_enabled = 0,
    telegram_enabled = 0,
    web_push_enabled = 0
WHERE user_id = ?
"#,
    )
    .bind("u_region_scope")
    .execute(&db)
    .await
    .unwrap();
    catnap::db::set_monitoring_partition_enabled(&db, "u_region_scope", "2", None, true)
        .await
        .unwrap();

    let state = build_state(cfg.clone(), db.clone()).await;
    catnap::poller::probe_catalog_topology(&state, "test")
        .await
        .unwrap();

    let targets = catnap::db::list_catalog_task_keys(&db).await.unwrap();
    assert_eq!(
        targets,
        vec![
            ("2".to_string(), None),
            ("2".to_string(), Some("56".to_string())),
            ("2".to_string(), Some("57".to_string())),
        ]
    );

    let row = sqlx::query("SELECT name, source_gid FROM catalog_configs WHERE id = ?")
        .bind("lc:2:57:257")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>(0), "HKG Premium Plus");
    assert_eq!(row.get::<Option<String>, _>(1), Some("57".to_string()));

    let row = sqlx::query("SELECT lifecycle_state FROM catalog_configs WHERE id = ?")
        .bind(&configs[0].id)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>(0), "active");

    let rows = sqlx::query(
        "SELECT user_id, scope FROM event_logs WHERE scope LIKE 'catalog.%' ORDER BY user_id ASC, scope ASC",
    )
    .fetch_all(&db)
    .await
    .unwrap();
    let actual = rows
        .into_iter()
        .map(|row| (row.get::<String, _>(0), row.get::<String, _>(1)))
        .collect::<Vec<_>>();
    assert_eq!(
        actual,
        vec![(
            "u_region_scope".to_string(),
            "catalog.partition.added".to_string(),
        )]
    );
}

#[tokio::test]
async fn probe_catalog_topology_prefetches_country_root_configs_for_new_country() {
    let root_html = r#"
<!doctype html>
<div class="firstgroup_item" onclick="window.location.href='/cart?fid=2'">
  <span class="yy-bth-text-a">CN</span>
</div>
"#;
    let fid_html = r#"
<!doctype html>
<html lang="zh-CN">
  <body>
    <div class="firstgroup_box_group">
      <div class="secondgroup_item pointer active" onclick="window.location.href='/cart?fid=2&gid=56'">
        <a class="yy-bth-text-a">HKG Premium</a>
        <a class="yy-bth-text-b">湾仔</a>
      </div>
    </div>
    <div class="card cartitem shadow w-100">
      <div class="card-body">
        <h4>CN Direct Premium</h4>
      </div>
      <div class="ml-4">
        <p class="card-text">库存： 3</p>
      </div>
      <div class="text-right">
        ¥ <a class="cart-num DINCondensed-Bold">66.00</a> 元 / 月
      </div>
      <div class="card-footer">
        <a href="/cart?action=configureproduct&pid=256">立即购买</a>
      </div>
    </div>
  </body>
</html>
"#;
    let gid_56_html = include_str!("fixtures/cart-fid-2-gid-56.html");

    #[derive(serde::Deserialize)]
    struct CartQuery {
        fid: Option<String>,
        gid: Option<String>,
    }

    let upstream = Router::new().route(
        "/cart",
        axum::routing::get(
            move |axum::extract::Query(q): axum::extract::Query<CartQuery>| async move {
                match (q.fid.as_deref(), q.gid.as_deref()) {
                    (None, None) => root_html.to_string(),
                    (Some("2"), None) => fid_html.to_string(),
                    (Some("2"), Some("56")) => gid_56_html.to_string(),
                    _ => "not found".to_string(),
                }
            },
        ),
    );
    let base = spawn_stub_server(upstream).await;

    let mut cfg = test_config();
    cfg.upstream_cart_url = format!("{base}/cart");

    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&cfg.db_url)
        .await
        .unwrap();
    catnap::db::init_db(&db).await.unwrap();

    let state = build_state(cfg.clone(), db.clone()).await;
    catnap::poller::probe_catalog_topology(&state, "test")
        .await
        .unwrap();

    let targets = catnap::db::list_catalog_task_keys(&db).await.unwrap();
    assert_eq!(
        targets,
        vec![
            ("2".to_string(), None),
            ("2".to_string(), Some("56".to_string())),
        ]
    );

    let row = sqlx::query("SELECT name, source_gid FROM catalog_configs WHERE id = ?")
        .bind("lc:2:0:256")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>(0), "CN Direct Premium");
    assert_eq!(row.get::<Option<String>, _>(1), None);
}

#[tokio::test]
async fn probe_catalog_topology_keeps_country_direct_summary_when_region_prefetch_fails() {
    let root_html = r#"
<!doctype html>
<div class="firstgroup_item" onclick="window.location.href='/cart?fid=2'">
  <span class="yy-bth-text-a">CN</span>
</div>
"#;
    let fid_html = r#"
<!doctype html>
<html lang="zh-CN">
  <body>
    <div class="firstgroup_box_group">
      <div class="secondgroup_item pointer active" onclick="window.location.href='/cart?fid=2&gid=56'">
        <a class="yy-bth-text-a">HKG Premium</a>
        <a class="yy-bth-text-b">湾仔</a>
      </div>
    </div>
    <div class="card cartitem shadow w-100">
      <div class="card-body">
        <h4>CN Direct Premium</h4>
      </div>
      <div class="ml-4">
        <p class="card-text">库存： 3</p>
      </div>
      <div class="text-right">
        ¥ <a class="cart-num DINCondensed-Bold">66.00</a> 元 / 月
      </div>
      <div class="card-footer">
        <a href="/cart?action=configureproduct&pid=256">立即购买</a>
      </div>
    </div>
  </body>
</html>
"#;

    #[derive(serde::Deserialize)]
    struct CartQuery {
        fid: Option<String>,
        gid: Option<String>,
    }

    let upstream = Router::new().route(
        "/cart",
        axum::routing::get(
            move |axum::extract::Query(q): axum::extract::Query<CartQuery>| async move {
                match (q.fid.as_deref(), q.gid.as_deref()) {
                    (None, None) => (StatusCode::OK, root_html.to_string()),
                    (Some("2"), None) => (StatusCode::OK, fid_html.to_string()),
                    (Some("2"), Some("56")) => (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "temporary upstream issue".to_string(),
                    ),
                    _ => (StatusCode::NOT_FOUND, "not found".to_string()),
                }
            },
        ),
    );
    let base = spawn_stub_server(upstream).await;

    let mut cfg = test_config();
    cfg.upstream_cart_url = format!("{base}/cart");

    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&cfg.db_url)
        .await
        .unwrap();
    catnap::db::init_db(&db).await.unwrap();
    catnap::db::ensure_user(&db, &cfg, "u_site_scope")
        .await
        .unwrap();
    sqlx::query(
        r#"
UPDATE settings
SET monitoring_events_partition_catalog_change_enabled = 0,
    monitoring_events_region_partition_change_enabled = 0,
    monitoring_events_site_region_change_enabled = 1,
    telegram_enabled = 0,
    web_push_enabled = 0
WHERE user_id = ?
"#,
    )
    .bind("u_site_scope")
    .execute(&db)
    .await
    .unwrap();

    let state = build_state(cfg.clone(), db.clone()).await;
    catnap::poller::probe_catalog_topology(&state, "test")
        .await
        .unwrap();

    let row = sqlx::query("SELECT name, source_gid FROM catalog_configs WHERE id = ?")
        .bind("lc:2:0:256")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>(0), "CN Direct Premium");
    assert_eq!(row.get::<Option<String>, _>(1), None);

    let record = sqlx::query(
        "SELECT summary FROM notification_records WHERE user_id = ? AND kind = ? ORDER BY id DESC LIMIT 1",
    )
    .bind("u_site_scope")
    .bind("catalog.region.added")
    .fetch_one(&db)
    .await
    .unwrap();
    let summary = record.get::<String, _>(0);
    assert!(summary.contains("已抓到 1 个套餐"));
    assert!(summary.contains("部分摘要抓取失败"));
}

#[tokio::test]
async fn probe_catalog_topology_marks_added_region_summary_fetch_failed_for_ambiguous_empty_page() {
    let root_html = r#"
<!doctype html>
<div class="firstgroup_item" onclick="window.location.href='/cart?fid=2'">
  <span class="yy-bth-text-a">CN</span>
</div>
"#;
    let fid_html = r#"
<!doctype html>
<html lang="zh-CN">
  <body>
    <div class="firstgroup_box_group">
      <div class="secondgroup_item pointer active" onclick="window.location.href='/cart?fid=2&gid=57'">
        <a class="yy-bth-text-a">HKG Premium Plus</a>
        <a class="yy-bth-text-b">九龙</a>
      </div>
    </div>
  </body>
</html>
"#;
    let gid_57_html = "<!doctype html><html><body>temporary upstream issue</body></html>";

    #[derive(serde::Deserialize)]
    struct CartQuery {
        fid: Option<String>,
        gid: Option<String>,
    }

    let upstream = Router::new().route(
        "/cart",
        axum::routing::get(
            move |axum::extract::Query(q): axum::extract::Query<CartQuery>| async move {
                match (q.fid.as_deref(), q.gid.as_deref()) {
                    (None, None) => (StatusCode::OK, root_html.to_string()),
                    (Some("2"), None) => (StatusCode::OK, fid_html.to_string()),
                    (Some("2"), Some("57")) => (StatusCode::OK, gid_57_html.to_string()),
                    _ => (StatusCode::NOT_FOUND, "not found".to_string()),
                }
            },
        ),
    );
    let base = spawn_stub_server(upstream).await;

    let mut cfg = test_config();
    cfg.upstream_cart_url = format!("{base}/cart");

    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&cfg.db_url)
        .await
        .unwrap();
    catnap::db::init_db(&db).await.unwrap();
    catnap::db::replace_catalog_topology(
        &db,
        &cfg.upstream_cart_url,
        &[catnap::models::Country {
            id: "2".to_string(),
            name: "CN".to_string(),
        }],
        &[],
    )
    .await
    .unwrap();
    catnap::db::ensure_user(&db, &cfg, "u_region_scope")
        .await
        .unwrap();
    sqlx::query(
        r#"
UPDATE settings
SET monitoring_events_partition_catalog_change_enabled = 0,
    monitoring_events_region_partition_change_enabled = 1,
    monitoring_events_site_region_change_enabled = 0,
    telegram_enabled = 0,
    web_push_enabled = 0
WHERE user_id = ?
"#,
    )
    .bind("u_region_scope")
    .execute(&db)
    .await
    .unwrap();
    catnap::db::set_monitoring_partition_enabled(&db, "u_region_scope", "2", None, true)
        .await
        .unwrap();

    let state = build_state(cfg.clone(), db.clone()).await;
    catnap::poller::probe_catalog_topology(&state, "test")
        .await
        .unwrap();

    let targets = catnap::db::list_catalog_task_keys(&db).await.unwrap();
    assert_eq!(
        targets,
        vec![
            ("2".to_string(), None),
            ("2".to_string(), Some("57".to_string())),
        ]
    );

    let cache_count = sqlx::query("SELECT COUNT(*) FROM catalog_url_cache WHERE url_key = '2:57'")
        .fetch_one(&db)
        .await
        .unwrap()
        .get::<i64, _>(0);
    assert_eq!(cache_count, 0);

    let record = sqlx::query(
        "SELECT summary FROM notification_records WHERE user_id = ? AND kind = ? ORDER BY id DESC LIMIT 1",
    )
    .bind("u_region_scope")
    .bind("catalog.partition.added")
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(
        record.get::<String, _>(0),
        "CN / HKG Premium Plus｜套餐摘要抓取失败，稍后重试"
    );
}

#[tokio::test]
async fn probe_catalog_topology_skips_country_root_configs_when_root_page_only_mirrors_region() {
    let root_html = r#"
<!doctype html>
<div class="firstgroup_item" onclick="window.location.href='/cart?fid=2'">
  <span class="yy-bth-text-a">CN</span>
</div>
"#;
    let fid_html = r#"
<!doctype html>
<html lang="zh-CN">
  <body>
    <div class="firstgroup_box_group">
      <div class="secondgroup_item pointer active" onclick="window.location.href='/cart?fid=2&gid=56'">
        <a class="yy-bth-text-a">HKG Premium</a>
        <a class="yy-bth-text-b">湾仔</a>
      </div>
    </div>
    <div class="card cartitem shadow w-100">
      <div class="card-body">
        <h4>HKG-Premium Basic</h4>
        <div class="card-text mb-4 mt-3">
          <li>CPU：<b>2核</b></li>
          <li>内存：<b>4G</b></li>
        </div>
      </div>
      <div class="text-right">
        ¥ <a class="cart-num DINCondensed-Bold">24.90</a> 元 / 月
      </div>
      <div class="card-footer">
        <a href="/cart?action=configureproduct&pid=117">立即购买</a>
      </div>
    </div>
  </body>
</html>
"#;
    let gid_56_html = include_str!("fixtures/cart-fid-2-gid-56.html");

    #[derive(serde::Deserialize)]
    struct CartQuery {
        fid: Option<String>,
        gid: Option<String>,
    }

    let upstream = Router::new().route(
        "/cart",
        axum::routing::get(
            move |axum::extract::Query(q): axum::extract::Query<CartQuery>| async move {
                match (q.fid.as_deref(), q.gid.as_deref()) {
                    (None, None) => root_html.to_string(),
                    (Some("2"), None) => fid_html.to_string(),
                    (Some("2"), Some("56")) => gid_56_html.to_string(),
                    _ => "not found".to_string(),
                }
            },
        ),
    );
    let base = spawn_stub_server(upstream).await;

    let mut cfg = test_config();
    cfg.upstream_cart_url = format!("{base}/cart");

    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&cfg.db_url)
        .await
        .unwrap();
    catnap::db::init_db(&db).await.unwrap();

    let state = build_state(cfg.clone(), db.clone()).await;
    catnap::poller::probe_catalog_topology(&state, "test")
        .await
        .unwrap();

    let targets = catnap::db::list_catalog_task_keys(&db).await.unwrap();
    assert_eq!(
        targets,
        vec![
            ("2".to_string(), None),
            ("2".to_string(), Some("56".to_string())),
        ]
    );

    let direct_count = sqlx::query(
        "SELECT COUNT(*) FROM catalog_configs WHERE source_fid = '2' AND source_gid IS NULL",
    )
    .fetch_one(&db)
    .await
    .unwrap()
    .get::<i64, _>(0);
    assert_eq!(direct_count, 0);
}

#[tokio::test]
async fn refresh_catalog_topology_preserves_regions_when_country_page_is_ambiguous() {
    let root_html = r#"
<!doctype html>
<div class="firstgroup_item" onclick="window.location.href='/cart?fid=2'">
  <span class="yy-bth-text-a">CN</span>
</div>
"#;
    let ambiguous_fid_html = "<!doctype html><html><body>temporary upstream issue</body></html>";

    #[derive(serde::Deserialize)]
    struct CartQuery {
        fid: Option<String>,
    }

    let upstream = Router::new().route(
        "/cart",
        axum::routing::get(
            move |axum::extract::Query(q): axum::extract::Query<CartQuery>| async move {
                match q.fid.as_deref() {
                    None => root_html,
                    Some("2") => ambiguous_fid_html,
                    Some(_) => "not found",
                }
            },
        ),
    );
    let base = spawn_stub_server(upstream).await;

    let mut cfg = test_config();
    cfg.upstream_cart_url = format!("{base}/cart");

    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&cfg.db_url)
        .await
        .unwrap();
    catnap::db::init_db(&db).await.unwrap();

    let countries = vec![catnap::models::Country {
        id: "2".to_string(),
        name: "CN".to_string(),
    }];
    let regions = vec![catnap::models::Region {
        id: "56".to_string(),
        country_id: "2".to_string(),
        name: "HKG Premium".to_string(),
        location_name: Some("湾仔".to_string()),
    }];
    catnap::db::replace_catalog_topology(&db, &cfg.upstream_cart_url, &countries, &regions)
        .await
        .unwrap();
    catnap::db::set_catalog_region_notice(&db, "2", None, Some("country root notice"))
        .await
        .unwrap();

    let mut configs = catnap::upstream::parse_configs(
        "2",
        Some("56"),
        include_str!("fixtures/cart-fid-2-gid-56.html"),
    );
    configs.truncate(1);
    catnap::db::upsert_catalog_configs(&db, &configs)
        .await
        .unwrap();

    for (user_id, region_enabled, site_enabled) in [
        ("u_region_scope", true, false),
        ("u_site_scope", false, true),
    ] {
        catnap::db::ensure_user(&db, &cfg, user_id).await.unwrap();
        sqlx::query(
            r#"
UPDATE settings
SET monitoring_events_partition_catalog_change_enabled = 0,
    monitoring_events_region_partition_change_enabled = ?,
    monitoring_events_site_region_change_enabled = ?,
    telegram_enabled = 0,
    web_push_enabled = 0
WHERE user_id = ?
"#,
        )
        .bind(if region_enabled { 1 } else { 0 })
        .bind(if site_enabled { 1 } else { 0 })
        .bind(user_id)
        .execute(&db)
        .await
        .unwrap();
    }
    catnap::db::set_monitoring_partition_enabled(&db, "u_region_scope", "2", None, true)
        .await
        .unwrap();

    let state = build_state(cfg.clone(), db.clone()).await;
    catnap::poller::refresh_catalog_topology(&state, "test")
        .await
        .unwrap();

    let known_targets = catnap::db::list_known_catalog_targets(&db).await.unwrap();
    assert_eq!(
        known_targets,
        vec![("2".to_string(), Some("56".to_string()))]
    );

    let row = sqlx::query("SELECT lifecycle_state FROM catalog_configs WHERE id = ?")
        .bind(&configs[0].id)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>(0), "active");

    let row = sqlx::query("SELECT has_regions FROM catalog_countries WHERE id = '2'")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(row.get::<i64, _>(0), 1);

    let row = sqlx::query("SELECT COUNT(*) FROM catalog_regions WHERE country_id = '2'")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(row.get::<i64, _>(0), 1);

    let root_notice = sqlx::query("SELECT text FROM catalog_region_notices WHERE url_key = '2:0'")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(root_notice.get::<String, _>(0), "country root notice");

    let rows = sqlx::query("SELECT COUNT(*) FROM event_logs WHERE scope LIKE 'catalog.%'")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(rows.get::<i64, _>(0), 0);
}
