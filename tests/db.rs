use catnap::RuntimeConfig;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::Row;

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
  source_pid, source_fid, source_gid
) VALUES (?, ?, NULL, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'active', ?, NULL, ?, NULL, ?, ?)
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
