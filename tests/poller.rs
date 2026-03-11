use axum::Router;
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
    let fid_html = include_str!("fixtures/cart-fid-2.html");

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
                    Some("2") => fid_html,
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
            ("2".to_string(), Some("56".to_string())),
            ("2".to_string(), Some("57".to_string())),
        ]
    );

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

    let rows = sqlx::query("SELECT COUNT(*) FROM event_logs WHERE scope LIKE 'catalog.%'")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(rows.get::<i64, _>(0), 0);
}
