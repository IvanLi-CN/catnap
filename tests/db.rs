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
        lazycat_base_url: "https://lxc.lazycat.wiki".to_string(),
        lazycat_site_sync_interval_minutes: 5,
        lazycat_panel_sync_interval_minutes: 10,
        lazycat_panel_concurrency: 2,
        lazycat_panel_timeout_ms: 5_000,
        lazycat_allow_invalid_tls: true,
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
        catnap::db::CatalogUrlFetchHints {
            region_notice: None,
            empty_result_authoritative: false,
        },
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
async fn empty_parse_allows_delisting_country_root_when_country_has_regions() {
    let cfg = test_config();
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&cfg.db_url)
        .await
        .unwrap();

    catnap::db::init_db(&db).await.unwrap();
    catnap::db::replace_catalog_topology(
        &db,
        "https://example.invalid/cart",
        &[catnap::models::Country {
            id: "2".to_string(),
            name: "CN".to_string(),
        }],
        &[catnap::models::Region {
            id: "56".to_string(),
            country_id: "2".to_string(),
            name: "Hong Kong".to_string(),
            location_name: Some("Hong Kong".to_string()),
        }],
    )
    .await
    .unwrap();

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
) VALUES (?, ?, NULL, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'active', ?, NULL, ?, ?, NULL, ?, NULL)
"#,
    )
    .bind("cfg_country_root")
    .bind("2")
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
    .execute(&db)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO catalog_url_cache (url_key, url, config_ids_json, last_success_at, updated_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("2:0")
    .bind("https://example.invalid/cart?fid=2")
    .bind("[\"cfg_country_root\"]")
    .bind("2026-01-01T00:00:00Z")
    .bind("2026-01-01T00:00:00Z")
    .execute(&db)
    .await
    .unwrap();

    let res = catnap::db::apply_catalog_url_fetch_success(
        &db,
        "2",
        None,
        "2:0",
        "https://example.invalid/cart?fid=2",
        vec![],
        catnap::db::CatalogUrlFetchHints {
            region_notice: None,
            empty_result_authoritative: true,
        },
    )
    .await
    .unwrap();
    assert_eq!(res.delisted_ids, vec!["cfg_country_root".to_string()]);

    let row = sqlx::query("SELECT lifecycle_state FROM catalog_configs WHERE id = ?")
        .bind("cfg_country_root")
        .fetch_one(&db)
        .await
        .unwrap();
    let state: String = row.get(0);
    assert_eq!(state, "delisted");
}

#[tokio::test]
async fn empty_parse_still_refuses_country_root_without_authoritative_empty() {
    let cfg = test_config();
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&cfg.db_url)
        .await
        .unwrap();

    catnap::db::init_db(&db).await.unwrap();
    catnap::db::replace_catalog_topology(
        &db,
        "https://example.invalid/cart",
        &[catnap::models::Country {
            id: "2".to_string(),
            name: "CN".to_string(),
        }],
        &[catnap::models::Region {
            id: "56".to_string(),
            country_id: "2".to_string(),
            name: "Hong Kong".to_string(),
            location_name: Some("Hong Kong".to_string()),
        }],
    )
    .await
    .unwrap();

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
) VALUES (?, ?, NULL, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'active', ?, NULL, ?, ?, NULL, ?, NULL)
"#,
    )
    .bind("cfg_country_root")
    .bind("2")
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
    .execute(&db)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO catalog_url_cache (url_key, url, config_ids_json, last_success_at, updated_at) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("2:0")
    .bind("https://example.invalid/cart?fid=2")
    .bind("[\"cfg_country_root\"]")
    .bind("2026-01-01T00:00:00Z")
    .bind("2026-01-01T00:00:00Z")
    .execute(&db)
    .await
    .unwrap();

    let err = catnap::db::apply_catalog_url_fetch_success(
        &db,
        "2",
        None,
        "2:0",
        "https://example.invalid/cart?fid=2",
        vec![],
        catnap::db::CatalogUrlFetchHints {
            region_notice: None,
            empty_result_authoritative: false,
        },
    )
    .await
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("refusing to apply empty catalog config list"),
        "unexpected error: {msg}"
    );
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
        catnap::db::CatalogUrlFetchHints {
            region_notice: Some("第一条说明"),
            empty_result_authoritative: false,
        },
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
        catnap::db::CatalogUrlFetchHints {
            region_notice: None,
            empty_result_authoritative: false,
        },
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

    // Simulate a restart while the config is still waiting for first stock.
    catnap::db::init_db(&db).await.unwrap();

    let restarted_row =
        sqlx::query("SELECT lifecycle_listed_event_at FROM catalog_configs WHERE id = ?")
            .bind(&config.id)
            .fetch_one(&db)
            .await
            .unwrap();
    assert!(restarted_row.get::<Option<String>, _>(0).is_none());

    config.inventory.quantity = 2;
    config.inventory.status = "in_stock".to_string();
    let res2 = catnap::db::apply_catalog_url_fetch_success(
        &db,
        "7",
        Some("40"),
        "7:40",
        "https://example.invalid/cart?fid=7&gid=40",
        vec![config.clone()],
        catnap::db::CatalogUrlFetchHints {
            region_notice: None,
            empty_result_authoritative: false,
        },
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
        catnap::db::CatalogUrlFetchHints {
            region_notice: None,
            empty_result_authoritative: false,
        },
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

#[tokio::test]
async fn country_root_notice_state_stays_active_when_country_has_regions() {
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
    catnap::db::set_catalog_region_notice(&db, "2", None, Some("country root notice"))
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

    // Simulate topology persistence across restart/refresh.
    catnap::db::replace_catalog_topology(&db, "https://example.invalid/cart", &countries, &regions)
        .await
        .unwrap();

    let notice_row = sqlx::query("SELECT text FROM catalog_region_notices WHERE url_key = ?")
        .bind("2:0")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(notice_row.get::<String, _>(0), "country root notice");

    let snapshot = catnap::db::load_catalog_snapshot(&db, &cfg.upstream_cart_url)
        .await
        .unwrap();
    assert!(snapshot.region_notice_initialized_keys.contains("2:0"));
}

#[tokio::test]
async fn cleanup_notification_records_applies_day_and_row_limits() {
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
    let item = catnap::db::load_notification_record_item_snapshot(&db, &configs[0].id)
        .await
        .unwrap()
        .unwrap();

    let old_id = catnap::db::insert_notification_record(
        &db,
        "u_1",
        &catnap::models::NotificationRecordDraft {
            kind: "monitoring.config".to_string(),
            title: "old".to_string(),
            summary: "old".to_string(),
            partition_label: item.partition_label.clone(),
            telegram_status: "success".to_string(),
            web_push_status: "skipped".to_string(),
            items: vec![item.clone()],
        },
    )
    .await
    .unwrap();
    let mid_id = catnap::db::insert_notification_record(
        &db,
        "u_1",
        &catnap::models::NotificationRecordDraft {
            kind: "monitoring.config".to_string(),
            title: "mid".to_string(),
            summary: "mid".to_string(),
            partition_label: item.partition_label.clone(),
            telegram_status: "success".to_string(),
            web_push_status: "skipped".to_string(),
            items: vec![item.clone()],
        },
    )
    .await
    .unwrap();
    let new_id = catnap::db::insert_notification_record(
        &db,
        "u_1",
        &catnap::models::NotificationRecordDraft {
            kind: "monitoring.config".to_string(),
            title: "new".to_string(),
            summary: "new".to_string(),
            partition_label: item.partition_label.clone(),
            telegram_status: "success".to_string(),
            web_push_status: "skipped".to_string(),
            items: vec![item.clone()],
        },
    )
    .await
    .unwrap();
    let other_user_id = catnap::db::insert_notification_record(
        &db,
        "u_2",
        &catnap::models::NotificationRecordDraft {
            kind: "monitoring.config".to_string(),
            title: "other".to_string(),
            summary: "other".to_string(),
            partition_label: item.partition_label.clone(),
            telegram_status: "success".to_string(),
            web_push_status: "skipped".to_string(),
            items: vec![item.clone()],
        },
    )
    .await
    .unwrap();

    sqlx::query("UPDATE notification_records SET created_at = ? WHERE id = ?")
        .bind("2020-01-01T00:00:00.000000000Z")
        .bind(&old_id)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("UPDATE notification_records SET created_at = ? WHERE id = ?")
        .bind("2026-03-10T00:00:00.000000000Z")
        .bind(&mid_id)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("UPDATE notification_records SET created_at = ? WHERE id = ?")
        .bind("2026-03-11T00:00:00.000000000Z")
        .bind(&new_id)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("UPDATE notification_records SET created_at = ? WHERE id = ?")
        .bind("2026-03-09T00:00:00.000000000Z")
        .bind(&other_user_id)
        .execute(&db)
        .await
        .unwrap();

    catnap::db::cleanup_notification_records(&db, 30, 1)
        .await
        .unwrap();

    let rows = sqlx::query(
        "SELECT user_id, id FROM notification_records ORDER BY user_id ASC, created_at DESC, id DESC",
    )
        .fetch_all(&db)
        .await
        .unwrap();
    let ids = rows
        .into_iter()
        .map(|row| (row.get::<String, _>(0), row.get::<String, _>(1)))
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        vec![
            ("u_1".to_string(), new_id),
            ("u_2".to_string(), other_user_id),
        ]
    );

    let orphan_count = sqlx::query("SELECT COUNT(*) FROM notification_record_items")
        .fetch_one(&db)
        .await
        .unwrap()
        .get::<i64, _>(0);
    assert_eq!(orphan_count, 2);
}

#[tokio::test]
async fn notification_records_preserve_item_order() {
    let cfg = test_config();
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&cfg.db_url)
        .await
        .unwrap();
    catnap::db::init_db(&db).await.unwrap();

    let mut configs =
        catnap::upstream::parse_configs("7", Some("40"), include_str!("fixtures/cart-fid-7.html"));
    configs.truncate(2);
    catnap::db::upsert_catalog_configs(&db, &configs)
        .await
        .unwrap();

    let items = catnap::db::load_notification_record_item_snapshots(
        &db,
        &configs
            .iter()
            .map(|config| config.id.clone())
            .collect::<Vec<_>>(),
    )
    .await
    .unwrap();
    assert_eq!(items.len(), 2);

    let record_id = catnap::db::insert_notification_record(
        &db,
        "u_1",
        &catnap::models::NotificationRecordDraft {
            kind: "catalog.partition_listed".to_string(),
            title: "grouped".to_string(),
            summary: "two items".to_string(),
            partition_label: items[0].partition_label.clone(),
            telegram_status: "success".to_string(),
            web_push_status: "skipped".to_string(),
            items: vec![items[1].clone(), items[0].clone()],
        },
    )
    .await
    .unwrap();

    let listed = catnap::db::list_notification_records(&db, "u_1", None, 20)
        .await
        .unwrap()
        .0;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, record_id);
    assert_eq!(listed[0].items.len(), 2);
    assert_eq!(
        listed[0].items[0].config_id.as_deref(),
        items[1].config_id.as_deref()
    );
    assert_eq!(
        listed[0].items[1].config_id.as_deref(),
        items[0].config_id.as_deref()
    );

    let detail = catnap::db::get_notification_record(&db, "u_1", &record_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(detail.items.len(), 2);
    assert_eq!(
        detail.items[0].config_id.as_deref(),
        items[1].config_id.as_deref()
    );
    assert_eq!(
        detail.items[1].config_id.as_deref(),
        items[0].config_id.as_deref()
    );
}

#[tokio::test]
async fn lazycat_traffic_samples_keep_latest_row_per_hour_bucket() {
    let cfg = test_config();
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&cfg.db_url)
        .await
        .unwrap();

    catnap::db::init_db(&db).await.unwrap();

    let first = catnap::db::LazycatTrafficSampleRecord {
        service_id: 2312,
        bucket_at: "2026-03-21T10:00:00Z".to_string(),
        sampled_at: "2026-03-21T10:05:00Z".to_string(),
        cycle_start_at: "2026-03-11T00:00:00Z".to_string(),
        cycle_end_at: "2026-04-11T00:00:00Z".to_string(),
        used_gb: 120.0,
        limit_gb: 800.0,
        reset_day: 11,
        last_reset_at: Some("2026-03-11T00:00:00Z".to_string()),
        display: Some("GB".to_string()),
    };
    let latest = catnap::db::LazycatTrafficSampleRecord {
        sampled_at: "2026-03-21T10:55:00Z".to_string(),
        used_gb: 138.6,
        ..first.clone()
    };

    catnap::db::upsert_lazycat_traffic_sample(&db, "u_1", &first)
        .await
        .unwrap();
    catnap::db::upsert_lazycat_traffic_sample(&db, "u_1", &latest)
        .await
        .unwrap();

    let rows = catnap::db::list_lazycat_traffic_samples(&db, "u_1")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].bucket_at, "2026-03-21T10:00:00Z");
    assert_eq!(rows[0].sampled_at, "2026-03-21T10:55:00Z");
    assert_eq!(rows[0].used_gb, 138.6);

    let next_cycle = catnap::db::LazycatTrafficSampleRecord {
        bucket_at: "2026-04-12T10:00:00Z".to_string(),
        sampled_at: "2026-04-12T10:10:00Z".to_string(),
        cycle_start_at: "2026-04-11T00:00:00Z".to_string(),
        cycle_end_at: "2026-05-11T00:00:00Z".to_string(),
        used_gb: 12.4,
        ..latest.clone()
    };
    catnap::db::upsert_lazycat_traffic_sample(&db, "u_1", &next_cycle)
        .await
        .unwrap();

    let current_cycle_rows = catnap::db::list_lazycat_traffic_samples_for_cycle(
        &db,
        "u_1",
        2312,
        "2026-03-11T00:00:00Z",
        "2026-04-11T00:00:00Z",
    )
    .await
    .unwrap();
    assert_eq!(current_cycle_rows.len(), 1);
    assert_eq!(current_cycle_rows[0].sampled_at, "2026-03-21T10:55:00Z");
}

#[tokio::test]
async fn lazycat_traffic_samples_keep_distinct_cycles_in_same_hour_bucket() {
    let cfg = test_config();
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&cfg.db_url)
        .await
        .unwrap();

    catnap::db::init_db(&db).await.unwrap();

    let previous_cycle = catnap::db::LazycatTrafficSampleRecord {
        service_id: 2312,
        bucket_at: "2026-03-21T10:00:00Z".to_string(),
        sampled_at: "2026-03-21T10:25:00Z".to_string(),
        cycle_start_at: "2026-02-21T10:30:00Z".to_string(),
        cycle_end_at: "2026-03-21T10:30:00Z".to_string(),
        used_gb: 799.2,
        limit_gb: 800.0,
        reset_day: 21,
        last_reset_at: Some("2026-02-21T10:30:00Z".to_string()),
        display: Some("GB".to_string()),
    };
    let next_cycle = catnap::db::LazycatTrafficSampleRecord {
        sampled_at: "2026-03-21T10:35:00Z".to_string(),
        cycle_start_at: "2026-03-21T10:30:00Z".to_string(),
        cycle_end_at: "2026-04-21T10:30:00Z".to_string(),
        used_gb: 0.4,
        last_reset_at: Some("2026-03-21T10:30:00Z".to_string()),
        ..previous_cycle.clone()
    };

    catnap::db::upsert_lazycat_traffic_sample(&db, "u_1", &previous_cycle)
        .await
        .unwrap();
    catnap::db::upsert_lazycat_traffic_sample(&db, "u_1", &next_cycle)
        .await
        .unwrap();

    let rows = catnap::db::list_lazycat_traffic_samples(&db, "u_1")
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);

    let previous_cycle_rows = catnap::db::list_lazycat_traffic_samples_for_cycle(
        &db,
        "u_1",
        2312,
        "2026-02-21T10:30:00Z",
        "2026-03-21T10:30:00Z",
    )
    .await
    .unwrap();
    assert_eq!(previous_cycle_rows.len(), 1);
    assert_eq!(previous_cycle_rows[0].sampled_at, "2026-03-21T10:25:00Z");

    let next_cycle_rows = catnap::db::list_lazycat_traffic_samples_for_cycle(
        &db,
        "u_1",
        2312,
        "2026-03-21T10:30:00Z",
        "2026-04-21T10:30:00Z",
    )
    .await
    .unwrap();
    assert_eq!(next_cycle_rows.len(), 1);
    assert_eq!(next_cycle_rows[0].sampled_at, "2026-03-21T10:35:00Z");
}

#[tokio::test]
async fn init_db_migrates_lazycat_traffic_samples_primary_key() {
    let cfg = test_config();
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&cfg.db_url)
        .await
        .unwrap();

    sqlx::query(
        r#"
CREATE TABLE lazycat_traffic_samples (
  user_id TEXT NOT NULL,
  service_id INTEGER NOT NULL,
  bucket_at TEXT NOT NULL,
  sampled_at TEXT NOT NULL,
  cycle_start_at TEXT NOT NULL,
  cycle_end_at TEXT NOT NULL,
  used_gb REAL NOT NULL,
  limit_gb REAL NOT NULL,
  reset_day INTEGER NOT NULL,
  last_reset_at TEXT NULL,
  display TEXT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (user_id, service_id, bucket_at)
)
"#,
    )
    .execute(&db)
    .await
    .unwrap();

    sqlx::query(
        r#"
INSERT INTO lazycat_traffic_samples (
    user_id,
    service_id,
    bucket_at,
    sampled_at,
    cycle_start_at,
    cycle_end_at,
    used_gb,
    limit_gb,
    reset_day,
    last_reset_at,
    display,
    created_at,
    updated_at
) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
"#,
    )
    .bind("u_1")
    .bind(2312_i64)
    .bind("2026-03-21T10:00:00Z")
    .bind("2026-03-21T10:25:00Z")
    .bind("2026-02-21T10:30:00Z")
    .bind("2026-03-21T10:30:00Z")
    .bind(799.2_f64)
    .bind(800.0_f64)
    .bind(21_i64)
    .bind(Some("2026-02-21T10:30:00Z"))
    .bind(Some("GB"))
    .bind("2026-03-21T10:25:00Z")
    .bind("2026-03-21T10:25:00Z")
    .execute(&db)
    .await
    .unwrap();

    catnap::db::init_db(&db).await.unwrap();

    let primary_key_rows = sqlx::query("PRAGMA table_info(lazycat_traffic_samples)")
        .fetch_all(&db)
        .await
        .unwrap();
    let mut primary_key_columns = primary_key_rows
        .into_iter()
        .filter_map(|row| {
            let ordinal = row.get::<i64, _>(5);
            if ordinal > 0 {
                Some((ordinal, row.get::<String, _>(1)))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    primary_key_columns.sort_by_key(|(ordinal, _)| *ordinal);
    let primary_key_columns = primary_key_columns
        .into_iter()
        .map(|(_, name)| name)
        .collect::<Vec<_>>();
    assert_eq!(
        primary_key_columns,
        vec!["user_id", "service_id", "cycle_start_at", "bucket_at"]
    );

    let rows = catnap::db::list_lazycat_traffic_samples(&db, "u_1")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].sampled_at, "2026-03-21T10:25:00Z");
}

#[tokio::test]
async fn lazycat_traffic_samples_for_cycles_batch_filters_requested_windows() {
    let cfg = test_config();
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&cfg.db_url)
        .await
        .unwrap();

    catnap::db::init_db(&db).await.unwrap();

    for sample in [
        catnap::db::LazycatTrafficSampleRecord {
            service_id: 2312,
            bucket_at: "2026-03-21T10:00:00Z".to_string(),
            sampled_at: "2026-03-21T10:25:00Z".to_string(),
            cycle_start_at: "2026-02-21T10:30:00Z".to_string(),
            cycle_end_at: "2026-03-21T10:30:00Z".to_string(),
            used_gb: 799.2,
            limit_gb: 800.0,
            reset_day: 21,
            last_reset_at: Some("2026-02-21T10:30:00Z".to_string()),
            display: Some("TiB".to_string()),
        },
        catnap::db::LazycatTrafficSampleRecord {
            service_id: 2312,
            bucket_at: "2026-03-21T11:00:00Z".to_string(),
            sampled_at: "2026-03-21T11:10:00Z".to_string(),
            cycle_start_at: "2026-03-21T10:30:00Z".to_string(),
            cycle_end_at: "2026-04-21T10:30:00Z".to_string(),
            used_gb: 0.9,
            limit_gb: 800.0,
            reset_day: 21,
            last_reset_at: Some("2026-03-21T10:30:00Z".to_string()),
            display: Some("TiB".to_string()),
        },
        catnap::db::LazycatTrafficSampleRecord {
            service_id: 5845,
            bucket_at: "2026-03-15T08:00:00Z".to_string(),
            sampled_at: "2026-03-15T08:10:00Z".to_string(),
            cycle_start_at: "2026-03-11T00:00:00Z".to_string(),
            cycle_end_at: "2026-04-11T00:00:00Z".to_string(),
            used_gb: 12.4,
            limit_gb: 200.0,
            reset_day: 11,
            last_reset_at: Some("2026-03-11T00:00:00Z".to_string()),
            display: Some("GB".to_string()),
        },
    ] {
        catnap::db::upsert_lazycat_traffic_sample(&db, "u_1", &sample)
            .await
            .unwrap();
    }

    let rows = catnap::db::list_lazycat_traffic_samples_for_cycles(
        &db,
        "u_1",
        &[
            (
                2312,
                "2026-03-21T10:30:00Z".to_string(),
                "2026-04-21T10:30:00Z".to_string(),
            ),
            (
                5845,
                "2026-03-11T00:00:00Z".to_string(),
                "2026-04-11T00:00:00Z".to_string(),
            ),
        ],
    )
    .await
    .unwrap();

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].service_id, 2312);
    assert_eq!(rows[0].cycle_start_at, "2026-03-21T10:30:00Z");
    assert_eq!(rows[1].service_id, 5845);
    assert_eq!(rows[1].cycle_start_at, "2026-03-11T00:00:00Z");
}

#[tokio::test]
async fn lazycat_latest_traffic_samples_for_services_returns_newest_rows() {
    let cfg = test_config();
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&cfg.db_url)
        .await
        .unwrap();

    catnap::db::init_db(&db).await.unwrap();

    for sample in [
        catnap::db::LazycatTrafficSampleRecord {
            service_id: 2312,
            bucket_at: "2026-03-21T10:00:00Z".to_string(),
            sampled_at: "2026-03-21T10:25:00Z".to_string(),
            cycle_start_at: "2026-03-11T00:00:00Z".to_string(),
            cycle_end_at: "2026-04-11T00:00:00Z".to_string(),
            used_gb: 120.0,
            limit_gb: 800.0,
            reset_day: 11,
            last_reset_at: Some("2026-03-11T00:00:00Z".to_string()),
            display: Some("GB".to_string()),
        },
        catnap::db::LazycatTrafficSampleRecord {
            service_id: 2312,
            bucket_at: "2026-03-21T11:00:00Z".to_string(),
            sampled_at: "2026-03-21T11:15:00Z".to_string(),
            cycle_start_at: "2026-03-11T00:00:00Z".to_string(),
            cycle_end_at: "2026-04-11T00:00:00Z".to_string(),
            used_gb: 140.0,
            limit_gb: 800.0,
            reset_day: 11,
            last_reset_at: Some("2026-03-11T00:00:00Z".to_string()),
            display: Some("GB".to_string()),
        },
        catnap::db::LazycatTrafficSampleRecord {
            service_id: 5845,
            bucket_at: "2026-03-15T08:00:00Z".to_string(),
            sampled_at: "2026-03-15T08:10:00Z".to_string(),
            cycle_start_at: "2026-03-01T00:00:00Z".to_string(),
            cycle_end_at: "2026-04-01T00:00:00Z".to_string(),
            used_gb: 12.4,
            limit_gb: 200.0,
            reset_day: 1,
            last_reset_at: Some("2026-03-01T00:00:00Z".to_string()),
            display: Some("TiB".to_string()),
        },
    ] {
        catnap::db::upsert_lazycat_traffic_sample(&db, "u_1", &sample)
            .await
            .unwrap();
    }

    let rows = catnap::db::list_latest_lazycat_traffic_samples_for_services(
        &db,
        "u_1",
        &[2312, 5845, 9999],
    )
    .await
    .unwrap();

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].service_id, 2312);
    assert_eq!(rows[0].sampled_at, "2026-03-21T11:15:00Z");
    assert_eq!(rows[1].service_id, 5845);
    assert_eq!(rows[1].sampled_at, "2026-03-15T08:10:00Z");
}
