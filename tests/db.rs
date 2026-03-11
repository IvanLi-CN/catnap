use catnap::RuntimeConfig;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::Row;

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
        upstream_cart_url: "https://lxc.lazycat.wiki/cart".to_string(),
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

#[tokio::test]
async fn init_db_does_not_reenable_disabled_auto_refresh() {
    let cfg = test_config();
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&cfg.db_url)
        .await
        .unwrap();

    catnap::db::init_db(&db).await.unwrap();

    let user_id = "u_1";
    catnap::db::ensure_user(&db, &cfg, user_id).await.unwrap();

    sqlx::query("UPDATE settings SET catalog_refresh_auto_interval_hours = NULL WHERE user_id = ?")
        .bind(user_id)
        .execute(&db)
        .await
        .unwrap();

    // Simulate service restart: init_db is called again.
    catnap::db::init_db(&db).await.unwrap();

    let row =
        sqlx::query("SELECT catalog_refresh_auto_interval_hours FROM settings WHERE user_id = ?")
            .bind(user_id)
            .fetch_one(&db)
            .await
            .unwrap();
    let v: Option<i64> = row.get(0);
    assert!(v.is_none(), "expected NULL to stay NULL, got {v:?}");
}

#[tokio::test]
async fn empty_parse_does_not_mass_delist_previous_ids() {
    let cfg = test_config();
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&cfg.db_url)
        .await
        .unwrap();

    catnap::db::init_db(&db).await.unwrap();

    // Seed one previously active config for fid=2/gid=56.
    sqlx::query(
        r#"
INSERT INTO catalog_configs (
  id, country_id, region_id, name, specs_json,
  price_amount, price_currency, price_period,
  inventory_status, inventory_quantity, checked_at,
  config_digest,
  lifecycle_state, lifecycle_listed_at, lifecycle_delisted_at, lifecycle_last_seen_at,
  lifecycle_listed_event_at,
  source_pid, source_fid, source_gid
) VALUES (?, ?, NULL, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'active', ?, NULL, ?, ?, NULL, ?, ?)
"#,
    )
    .bind("cfg_1")
    .bind("1")
    .bind("seed")
    .bind("{}")
    .bind(1.0_f64)
    .bind("USD")
    .bind("month")
    .bind("in_stock")
    .bind(1_i64)
    .bind("2026-01-01T00:00:00Z")
    .bind("digest")
    .bind("2026-01-01T00:00:00Z")
    .bind("2026-01-01T00:00:00Z")
    .bind("2026-01-01T00:00:00Z")
    .bind("2")
    .bind("56")
    .execute(&db)
    .await
    .unwrap();

    let err = catnap::db::apply_catalog_url_fetch_success(
        &db,
        "2",
        Some("56"),
        "2:56",
        "https://example.invalid/cart?fid=2&gid=56",
        vec![],
        None,
    )
    .await
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("refusing to apply empty catalog config list"),
        "unexpected error: {msg}"
    );

    let row = sqlx::query("SELECT lifecycle_state FROM catalog_configs WHERE id = ?")
        .bind("cfg_1")
        .fetch_one(&db)
        .await
        .unwrap();
    let state: String = row.get(0);
    assert_eq!(state, "active");
}

#[tokio::test]
async fn apply_catalog_url_fetch_success_persists_region_notice() {
    let cfg = test_config();
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&cfg.db_url)
        .await
        .unwrap();

    catnap::db::init_db(&db).await.unwrap();

    let mut configs =
        catnap::upstream::parse_configs("7", Some("40"), include_str!("fixtures/cart-fid-7.html"));
    configs.truncate(1);
    assert_eq!(configs.len(), 1);

    catnap::db::apply_catalog_url_fetch_success(
        &db,
        "7",
        Some("40"),
        "7:40",
        "https://example.invalid/cart?fid=7&gid=40",
        configs,
        Some("第一条说明"),
    )
    .await
    .unwrap();

    let row = sqlx::query("SELECT text FROM catalog_region_notices WHERE url_key = ?")
        .bind("7:40")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>(0), "第一条说明");
}

#[tokio::test]
async fn apply_catalog_url_fetch_success_delays_listed_until_first_positive_inventory() {
    let cfg = test_config();
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&cfg.db_url)
        .await
        .unwrap();

    catnap::db::init_db(&db).await.unwrap();

    let mut configs =
        catnap::upstream::parse_configs("7", Some("40"), include_str!("fixtures/cart-fid-7.html"));
    configs.truncate(1);
    let mut config = configs.remove(0);
    config.inventory.quantity = 0;
    config.inventory.status = "out_of_stock".to_string();

    let res = catnap::db::apply_catalog_url_fetch_success(
        &db,
        "7",
        Some("40"),
        "7:40",
        "https://example.invalid/cart?fid=7&gid=40",
        vec![config.clone()],
        None,
    )
    .await
    .unwrap();

    assert!(res.listed_ids.contains(&config.id));
    assert!(res.listed_pending_zero_stock_ids.contains(&config.id));
    assert!(res.listed_event_ids.is_empty());

    let row = sqlx::query("SELECT lifecycle_listed_event_at FROM catalog_configs WHERE id = ?")
        .bind(&config.id)
        .fetch_one(&db)
        .await
        .unwrap();
    assert!(row.get::<Option<String>, _>(0).is_none());

    config.inventory.quantity = 2;
    config.inventory.status = "in_stock".to_string();
    let res2 = catnap::db::apply_catalog_url_fetch_success(
        &db,
        "7",
        Some("40"),
        "7:40",
        "https://example.invalid/cart?fid=7&gid=40",
        vec![config.clone()],
        None,
    )
    .await
    .unwrap();

    assert!(res2.listed_ids.is_empty());
    assert!(res2.listed_pending_zero_stock_ids.is_empty());
    assert_eq!(res2.listed_event_ids, vec![config.id.clone()]);

    let row2 = sqlx::query("SELECT lifecycle_listed_event_at FROM catalog_configs WHERE id = ?")
        .bind(&config.id)
        .fetch_one(&db)
        .await
        .unwrap();
    assert!(row2.get::<Option<String>, _>(0).is_some());

    config.inventory.quantity = 5;
    let res3 = catnap::db::apply_catalog_url_fetch_success(
        &db,
        "7",
        Some("40"),
        "7:40",
        "https://example.invalid/cart?fid=7&gid=40",
        vec![config],
        None,
    )
    .await
    .unwrap();

    assert!(res3.listed_ids.is_empty());
    assert!(res3.listed_pending_zero_stock_ids.is_empty());
    assert!(res3.listed_event_ids.is_empty());
}

#[tokio::test]
async fn load_catalog_snapshot_ignores_stale_initialized_notice_keys() {
    let cfg = test_config();
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
    catnap::db::replace_catalog_topology(&db, "https://example.invalid/cart", &countries, &[])
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO catalog_url_cache (url_key, url, config_ids_json, last_success_at, updated_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("2:0")
    .bind("https://example.invalid/cart?fid=2")
    .bind("[]")
    .bind("2026-01-01T00:00:00Z")
    .bind("2026-01-01T00:00:00Z")
    .execute(&db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO catalog_url_cache (url_key, url, config_ids_json, last_success_at, updated_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("9:99")
    .bind("https://example.invalid/cart?fid=9&gid=99")
    .bind("[]")
    .bind("2026-01-01T00:00:00Z")
    .bind("2026-01-01T00:00:00Z")
    .execute(&db)
    .await
    .unwrap();

    let snapshot = catnap::db::load_catalog_snapshot(&db, &cfg.upstream_cart_url)
        .await
        .unwrap();

    assert!(snapshot.region_notice_initialized_keys.contains("2:0"));
    assert!(!snapshot.region_notice_initialized_keys.contains("9:99"));
}

#[tokio::test]
async fn retire_catalog_targets_delists_configs_and_hides_known_targets() {
    let cfg = test_config();
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&cfg.db_url)
        .await
        .unwrap();

    catnap::db::init_db(&db).await.unwrap();

    let mut configs =
        catnap::upstream::parse_configs("7", Some("40"), include_str!("fixtures/cart-fid-7.html"));
    configs.truncate(1);
    catnap::db::upsert_catalog_configs(&db, &configs)
        .await
        .unwrap();

    let known_before = catnap::db::list_known_catalog_targets(&db).await.unwrap();
    assert_eq!(
        known_before,
        vec![("7".to_string(), Some("40".to_string()))]
    );

    let retired =
        catnap::db::retire_catalog_targets(&db, &[("7".to_string(), Some("40".to_string()))])
            .await
            .unwrap();
    assert_eq!(retired, vec![configs[0].id.clone()]);

    let row = sqlx::query("SELECT lifecycle_state FROM catalog_configs WHERE id = ?")
        .bind(&configs[0].id)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>(0), "delisted");
    assert!(catnap::db::list_known_catalog_targets(&db)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn load_catalog_snapshot_round_trips_topology_state() {
    let cfg = test_config();
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
        name: "Hong Kong".to_string(),
        location_name: Some("Hong Kong".to_string()),
    }];
    catnap::db::replace_catalog_topology(&db, "https://example.invalid/cart", &countries, &regions)
        .await
        .unwrap();
    catnap::db::set_catalog_region_notice(&db, "2", Some("56"), Some("notice text"))
        .await
        .unwrap();

    let snapshot = catnap::db::load_catalog_snapshot(&db, &cfg.upstream_cart_url)
        .await
        .unwrap();

    assert_eq!(snapshot.source_url, "https://example.invalid/cart");
    assert_eq!(snapshot.topology_status, "success");
    assert!(snapshot.topology_refreshed_at.is_some());
    assert_eq!(snapshot.countries.len(), 1);
    assert_eq!(snapshot.regions.len(), 1);
    assert_eq!(snapshot.region_notices.len(), 1);
}
