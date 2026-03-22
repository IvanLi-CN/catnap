use axum::{
    body::{to_bytes, Body},
    http::{Method, Request, StatusCode},
};
use catnap::{build_app, AppState, RuntimeConfig};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::Row;
use sqlx::SqlitePool;
use std::collections::HashSet;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use time::format_description::well_known::Rfc3339;
use time::{Date, Duration, OffsetDateTime, PrimitiveDateTime, Time, UtcOffset};
use tower::ServiceExt;

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

struct TestApp {
    app: axum::Router,
    db: SqlitePool,
    state: AppState,
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
        region_notices: vec![catnap::models::RegionNotice {
            country_id: "2".to_string(),
            region_id: Some("56".to_string()),
            text: "HKG Premium 仅限合规使用，禁止滥用。".to_string(),
        }],
        region_notice_initialized_keys: std::collections::HashSet::from([String::from("2:56")]),
        configs: configs.clone(),
        fetched_at: "2026-01-19T00:00:00Z".to_string(),
        source_url: cfg.upstream_cart_url.clone(),
        topology_refreshed_at: Some("2026-01-19T00:00:00Z".to_string()),
        topology_request_count: 3,
        topology_status: "success".to_string(),
        topology_message: None,
    };

    catnap::db::replace_catalog_topology(
        &db,
        &cfg.upstream_cart_url,
        &snapshot.countries,
        &snapshot.regions,
    )
    .await
    .unwrap();
    catnap::db::upsert_catalog_configs(&db, &configs)
        .await
        .unwrap();
    catnap::db::replace_catalog_topology(
        &db,
        &cfg.upstream_cart_url,
        &snapshot.countries,
        &snapshot.regions,
    )
    .await
    .unwrap();
    for notice in &snapshot.region_notices {
        catnap::db::set_catalog_region_notice(
            &db,
            &notice.country_id,
            notice.region_id.as_deref(),
            Some(&notice.text),
        )
        .await
        .unwrap();
    }

    let catalog = std::sync::Arc::new(tokio::sync::RwLock::new(snapshot));
    let ops = catnap::ops::OpsManager::new(cfg.clone(), db.clone(), catalog.clone());
    ops.start();

    let state = AppState {
        config: cfg,
        db: db.clone(),
        catalog,
        catalog_refresh: catnap::catalog_refresh::CatalogRefreshManager::new(),
        ops,
        update_cache: catnap::update_check::new_cache(),
        lazycat_sync_users: Arc::new(tokio::sync::Mutex::new(HashSet::new())),
    };

    TestApp {
        app: build_app(state.clone()),
        db,
        state,
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

async fn ensure_user_exists(t: &TestApp, user_id: &str) {
    let res = t
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/bootstrap")
                .header("host", "example.com")
                .header("x-user", user_id)
                .header("origin", "http://example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

async fn save_telegram_settings(t: &TestApp, user_id: &str, bot_token: &str, target: &str) {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "poll": { "intervalMinutes": 1, "jitterPct": 0.1 },
        "siteBaseUrl": null,
        "notifications": {
            "telegram": { "enabled": true, "botToken": bot_token, "targets": [target] },
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
                .header("x-user", user_id)
                .header("origin", "http://example.com")
                .header("content-type", "application/json")
                .body(Body::from(bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

async fn post_telegram_test(
    t: &TestApp,
    user_id: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let bytes = serde_json::to_vec(&body).unwrap();
    let res = t
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/notifications/telegram/test")
                .header("host", "example.com")
                .header("x-user", user_id)
                .header("origin", "http://example.com")
                .header("content-type", "application/json")
                .body(Body::from(bytes))
                .unwrap(),
        )
        .await
        .unwrap();

    let status = res.status();
    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    (status, json)
}

async fn authed_json(
    t: &TestApp,
    user_id: &str,
    method: Method,
    uri: &str,
    body: Option<serde_json::Value>,
) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("host", "example.com")
        .header("x-user", user_id)
        .header("origin", "http://example.com");
    let request = if let Some(body) = body {
        builder = builder.header("content-type", "application/json");
        builder
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };

    let response = t.app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json = if bytes.is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, json)
}

async fn seed_lazycat_machine(
    t: &TestApp,
    user_id: &str,
    service_id: i64,
    email: &str,
    service_name: &str,
    primary_address: &str,
) {
    ensure_user_exists(t, user_id).await;
    let now = OffsetDateTime::now_utc().format(&Rfc3339).unwrap();

    let account = catnap::db::LazycatAccountRow {
        user_id: user_id.to_string(),
        email: email.to_string(),
        password: "secret".to_string(),
        cookies_json: Some(
            serde_json::to_string(&vec![(
                "PHPSESSID".to_string(),
                format!("sess-{service_id}"),
            )])
            .unwrap(),
        ),
        state: "ready".to_string(),
        last_error: None,
        last_authenticated_at: Some(now.clone()),
        last_site_sync_at: Some(now.clone()),
        last_panel_sync_at: Some(now.clone()),
        created_at: now.clone(),
        updated_at: now.clone(),
    };
    catnap::db::put_lazycat_account(&t.db, &account)
        .await
        .unwrap();

    let site = catnap::db::LazycatSiteMachineRecord {
        service_id,
        service_name: service_name.to_string(),
        service_code: format!("svc-{service_id}"),
        status: "Active".to_string(),
        os: Some("Debian 12".to_string()),
        primary_address: Some(primary_address.to_string()),
        extra_addresses: vec![format!("10.0.{}.2", service_id % 200)],
        billing_cycle: Some("monthly".to_string()),
        renew_price: Some("¥9.34元/月付".to_string()),
        first_price: Some("¥9.34元".to_string()),
        expires_at: Some("2026-04-01T00:00:00Z".to_string()),
        panel_kind: Some("container".to_string()),
        panel_url: Some(format!(
            "https://panel-{service_id}.example.test:8443/container/dashboard"
        )),
        panel_hash: Some(format!("hash-{service_id}")),
        last_site_sync_at: now.clone(),
    };
    catnap::db::upsert_lazycat_site_machines(&t.db, user_id, &[site])
        .await
        .unwrap();

    let detail = catnap::db::LazycatMachineDetailRecord {
        service_id,
        panel_kind: Some("container".to_string()),
        panel_url: Some(format!(
            "https://panel-{service_id}.example.test:8443/container/dashboard"
        )),
        panel_hash: Some(format!("hash-{service_id}")),
        traffic_used_gb: Some(123.4),
        traffic_limit_gb: Some(800.0),
        traffic_reset_day: Some(11),
        traffic_last_reset_at: Some("2026-03-11T00:00:00Z".to_string()),
        traffic_display: Some("123.4 GB / 800 GB".to_string()),
        detail_state: "ready".to_string(),
        detail_error: None,
        last_panel_sync_at: now.clone(),
    };
    catnap::db::update_lazycat_machine_detail(&t.db, user_id, &detail)
        .await
        .unwrap();

    for (bucket_at, sampled_at, used_gb) in [
        ("2026-03-18T00:00:00Z", "2026-03-18T00:20:00Z", 88.2),
        ("2026-03-19T00:00:00Z", "2026-03-19T00:20:00Z", 123.4),
    ] {
        let sample = catnap::db::LazycatTrafficSampleRecord {
            service_id,
            bucket_at: bucket_at.to_string(),
            sampled_at: sampled_at.to_string(),
            cycle_start_at: "2026-03-11T00:00:00Z".to_string(),
            cycle_end_at: "2026-04-11T00:00:00Z".to_string(),
            used_gb,
            limit_gb: 800.0,
            reset_day: 11,
            last_reset_at: Some("2026-03-11T00:00:00Z".to_string()),
            display: Some("GB".to_string()),
        };
        catnap::db::upsert_lazycat_traffic_sample(&t.db, user_id, &sample)
            .await
            .unwrap();
    }

    let mapping = catnap::db::LazycatPortMappingRecord {
        family: "v4".to_string(),
        mapping_key: format!("map-{service_id}"),
        public_ip: Some(primary_address.to_string()),
        public_port: Some(52000 + service_id),
        public_port_end: None,
        private_ip: Some("172.16.0.2".to_string()),
        private_port: Some(22),
        private_port_end: None,
        protocol: Some("tcp".to_string()),
        status: Some("enabled".to_string()),
        description: Some("ssh".to_string()),
        remote_created_at: Some(now.clone()),
        remote_updated_at: Some(now.clone()),
    };
    catnap::db::replace_lazycat_port_mappings(&t.db, user_id, service_id, "v4", &[mapping], &now)
        .await
        .unwrap();
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
    let notices = json["catalog"]["regionNotices"]
        .as_array()
        .expect("catalog.regionNotices should be an array");
    assert!(notices.iter().any(|n| {
        n["countryId"].as_str() == Some("2")
            && n["regionId"].as_str() == Some("56")
            && n["text"]
                .as_str()
                .is_some_and(|txt| txt.contains("禁止滥用"))
    }));
    assert_eq!(
        json["settings"]["catalogRefresh"]["autoIntervalHours"].as_i64(),
        Some(catnap::defaults::FIXED_CATALOG_TOPOLOGY_REFRESH_INTERVAL_HOURS)
    );
    assert!(json.get("settings").is_some());
    assert!(json.get("monitoring").is_some());
    assert_eq!(
        json["monitoring"]["enabledPartitions"]
            .as_array()
            .map(Vec::len),
        Some(0)
    );
    assert_eq!(
        json["settings"]["notifications"]["telegram"]["targets"]
            .as_array()
            .map(Vec::len),
        Some(0)
    );
    assert_eq!(json["lazycat"]["connected"].as_bool(), Some(false));
    assert_eq!(json["lazycat"]["machineCount"].as_i64(), Some(0));
    assert_eq!(json["lazycat"]["state"].as_str(), Some("disconnected"));
}

#[tokio::test]
async fn lazycat_login_rejects_empty_credentials() {
    let t = make_app().await;
    let (status, json) = authed_json(
        &t,
        "u_1",
        Method::POST,
        "/api/lazycat/account",
        Some(serde_json::json!({
            "email": " ",
            "password": "",
        })),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["error"]["code"].as_str(), Some("INVALID_ARGUMENT"));
    assert!(json["error"]["message"]
        .as_str()
        .is_some_and(|message| message.contains("不能为空")));
}

#[tokio::test]
async fn lazycat_login_rejects_when_sync_is_already_running() {
    let t = make_app().await;
    t.state
        .lazycat_sync_users
        .lock()
        .await
        .insert("u_1".to_string());

    let (status, json) = authed_json(
        &t,
        "u_1",
        Method::POST,
        "/api/lazycat/account",
        Some(serde_json::json!({
            "email": "user@example.com",
            "password": "secret",
        })),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["error"]["code"].as_str(), Some("INVALID_ARGUMENT"));
    assert!(json["error"]["message"]
        .as_str()
        .is_some_and(|message| message.contains("同步仍在进行")));
}

#[tokio::test]
async fn lazycat_machines_are_user_scoped_and_disconnect_cleans_current_user() {
    let t = make_app().await;
    seed_lazycat_machine(
        &t,
        "u_1",
        2312,
        "first@example.com",
        "港湾 Transit Mini",
        "edge-user-1.example.net",
    )
    .await;
    seed_lazycat_machine(
        &t,
        "u_2",
        3875,
        "second@example.com",
        "北湾 NAT Lite",
        "nat-user-2.example.net",
    )
    .await;

    let (bootstrap_status, bootstrap_json) =
        authed_json(&t, "u_1", Method::GET, "/api/bootstrap", None).await;
    assert_eq!(bootstrap_status, StatusCode::OK);
    assert_eq!(bootstrap_json["lazycat"]["connected"].as_bool(), Some(true));
    assert_eq!(
        bootstrap_json["lazycat"]["email"].as_str(),
        Some("first@example.com")
    );
    assert_eq!(bootstrap_json["lazycat"]["machineCount"].as_i64(), Some(1));

    let (status_u1, json_u1) =
        authed_json(&t, "u_1", Method::GET, "/api/lazycat/machines", None).await;
    assert_eq!(status_u1, StatusCode::OK);
    assert_eq!(
        json_u1["account"]["email"].as_str(),
        Some("first@example.com")
    );
    assert_eq!(json_u1["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(json_u1["items"][0]["serviceId"].as_i64(), Some(2312));
    assert_eq!(
        json_u1["items"][0]["portMappings"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(
        json_u1["items"][0]["traffic"]["resetDay"].as_i64(),
        Some(11)
    );
    assert_eq!(
        json_u1["items"][0]["traffic"]["cycleStartAt"].as_str(),
        Some("2026-03-11T00:00:00Z")
    );
    assert_eq!(
        json_u1["items"][0]["traffic"]["history"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    assert_eq!(json_u1["items"][0]["panelKind"].as_str(), Some("container"));
    assert_eq!(
        json_u1["items"][0]["panelUrl"].as_str(),
        Some("https://panel-2312.example.test:8443/container/dashboard")
    );

    let (status_u2, json_u2) =
        authed_json(&t, "u_2", Method::GET, "/api/lazycat/machines", None).await;
    assert_eq!(status_u2, StatusCode::OK);
    assert_eq!(
        json_u2["account"]["email"].as_str(),
        Some("second@example.com")
    );
    assert_eq!(json_u2["items"][0]["serviceId"].as_i64(), Some(3875));

    let (delete_status, delete_json) =
        authed_json(&t, "u_1", Method::DELETE, "/api/lazycat/account", None).await;
    assert_eq!(delete_status, StatusCode::OK);
    assert_eq!(delete_json["ok"].as_bool(), Some(true));

    let (account_status, account_json) =
        authed_json(&t, "u_1", Method::GET, "/api/lazycat/account", None).await;
    assert_eq!(account_status, StatusCode::OK);
    assert_eq!(account_json["connected"].as_bool(), Some(false));
    assert_eq!(account_json["state"].as_str(), Some("disconnected"));

    let (post_delete_status, post_delete_json) =
        authed_json(&t, "u_1", Method::GET, "/api/lazycat/machines", None).await;
    assert_eq!(post_delete_status, StatusCode::OK);
    assert_eq!(
        post_delete_json["account"]["connected"].as_bool(),
        Some(false)
    );
    assert_eq!(post_delete_json["items"].as_array().map(Vec::len), Some(0));

    let lazycat_account_count_u1 =
        sqlx::query("SELECT COUNT(*) FROM lazycat_accounts WHERE user_id = ?")
            .bind("u_1")
            .fetch_one(&t.db)
            .await
            .unwrap()
            .get::<i64, _>(0);
    let lazycat_machine_count_u1 =
        sqlx::query("SELECT COUNT(*) FROM lazycat_machines WHERE user_id = ?")
            .bind("u_1")
            .fetch_one(&t.db)
            .await
            .unwrap()
            .get::<i64, _>(0);
    let lazycat_mapping_count_u1 =
        sqlx::query("SELECT COUNT(*) FROM lazycat_port_mappings WHERE user_id = ?")
            .bind("u_1")
            .fetch_one(&t.db)
            .await
            .unwrap()
            .get::<i64, _>(0);
    let lazycat_traffic_sample_count_u1 =
        sqlx::query("SELECT COUNT(*) FROM lazycat_traffic_samples WHERE user_id = ?")
            .bind("u_1")
            .fetch_one(&t.db)
            .await
            .unwrap()
            .get::<i64, _>(0);
    assert_eq!(lazycat_account_count_u1, 0);
    assert_eq!(lazycat_machine_count_u1, 0);
    assert_eq!(lazycat_mapping_count_u1, 0);
    assert_eq!(lazycat_traffic_sample_count_u1, 0);

    let (status_u2_after, json_u2_after) =
        authed_json(&t, "u_2", Method::GET, "/api/lazycat/machines", None).await;
    assert_eq!(status_u2_after, StatusCode::OK);
    assert_eq!(json_u2_after["account"]["connected"].as_bool(), Some(true));
    assert_eq!(json_u2_after["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(json_u2_after["items"][0]["serviceId"].as_i64(), Some(3875));
}

#[tokio::test]
async fn lazycat_machine_vnc_url_returns_live_console_url() {
    let received_hostname = Arc::new(Mutex::new(None::<String>));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base = format!("http://{addr}");
    let info_base = base.clone();
    let received_hostname_for_route = received_hostname.clone();
    let stub = axum::Router::new()
        .route(
            "/provision/custom/content",
            axum::routing::get(move || {
                let info_base = info_base.clone();
                async move {
                    axum::response::Html(format!(
                        r#"<a href="{info_base}/container/dashboard?hash=live-hash-2312">控制台</a>"#
                    ))
                }
            }),
        )
        .route(
            "/api/container/console/create-token",
            axum::routing::post(
                move |headers: axum::http::HeaderMap, body: String| {
                    let received_hostname = received_hostname_for_route.clone();
                    async move {
                        assert_eq!(
                            headers.get("x-container-hash").and_then(|value| value.to_str().ok()),
                            Some("live-hash-2312")
                        );
                        let payload: serde_json::Value = serde_json::from_str(&body).unwrap();
                        *received_hostname.lock().unwrap() =
                            payload["hostname"].as_str().map(str::to_string);
                        axum::Json(serde_json::json!({
                            "code": 200,
                            "data": { "token": "live-console-token-2312" }
                        }))
                    }
                },
            ),
        );
    tokio::spawn(async move {
        axum::serve(listener, stub).await.unwrap();
    });

    let mut cfg = test_config();
    cfg.lazycat_base_url = base.clone();
    let t = make_app_with_config(cfg).await;
    seed_lazycat_machine(
        &t,
        "u_1",
        2312,
        "first@example.com",
        "港湾 Transit Mini",
        "edge-user-1.example.net",
    )
    .await;

    let (status, json) = authed_json(
        &t,
        "u_1",
        Method::POST,
        "/api/lazycat/machines/2312/vnc-url",
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["kind"].as_str(), Some("console"));
    let expected_url = format!("{base}/console?token=live-console-token-2312");
    assert_eq!(
        json["url"].as_str(),
        Some(expected_url.as_str())
    );
    assert_eq!(
        received_hostname.lock().unwrap().as_deref(),
        Some("svc-2312")
    );
}

#[tokio::test]
async fn lazycat_machines_hide_stale_previous_cycle_traffic_after_rollover() {
    let t = make_app().await;
    let service_id = 2312;
    seed_lazycat_machine(
        &t,
        "u_1",
        service_id,
        "first@example.com",
        "港湾 Transit Mini",
        "edge-user-1.example.net",
    )
    .await;

    let now = OffsetDateTime::now_utc();
    let current_cycle_start = PrimitiveDateTime::new(
        Date::from_calendar_date(now.year(), now.month(), 1).unwrap(),
        Time::MIDNIGHT,
    )
    .assume_offset(UtcOffset::UTC);
    let previous_cycle_marker = current_cycle_start - Duration::days(1);
    let previous_cycle_start = PrimitiveDateTime::new(
        Date::from_calendar_date(
            previous_cycle_marker.year(),
            previous_cycle_marker.month(),
            1,
        )
        .unwrap(),
        Time::MIDNIGHT,
    )
    .assume_offset(UtcOffset::UTC);
    let sampled_at = previous_cycle_start + Duration::days(10) + Duration::minutes(20);
    let previous_cycle_start_rfc3339 = previous_cycle_start.format(&Rfc3339).unwrap();
    let current_cycle_start_rfc3339 = current_cycle_start.format(&Rfc3339).unwrap();
    let sampled_at_rfc3339 = sampled_at.format(&Rfc3339).unwrap();

    let detail = catnap::db::LazycatMachineDetailRecord {
        service_id,
        panel_kind: Some("container".to_string()),
        panel_url: Some(format!(
            "https://panel-{service_id}.example.test:8443/container/dashboard"
        )),
        panel_hash: Some(format!("hash-{service_id}")),
        traffic_used_gb: Some(321.0),
        traffic_limit_gb: Some(800.0),
        traffic_reset_day: Some(1),
        traffic_last_reset_at: Some(previous_cycle_start_rfc3339.clone()),
        traffic_display: Some("321 GB / 800 GB".to_string()),
        detail_state: "stale".to_string(),
        detail_error: Some("panel timeout".to_string()),
        last_panel_sync_at: sampled_at_rfc3339.clone(),
    };
    catnap::db::update_lazycat_machine_detail(&t.db, "u_1", &detail)
        .await
        .unwrap();

    sqlx::query("DELETE FROM lazycat_traffic_samples WHERE user_id = ? AND service_id = ?")
        .bind("u_1")
        .bind(service_id)
        .execute(&t.db)
        .await
        .unwrap();

    let sample = catnap::db::LazycatTrafficSampleRecord {
        service_id,
        bucket_at: sampled_at_rfc3339.clone(),
        sampled_at: sampled_at_rfc3339.clone(),
        cycle_start_at: previous_cycle_start_rfc3339.clone(),
        cycle_end_at: current_cycle_start_rfc3339.clone(),
        used_gb: 321.0,
        limit_gb: 800.0,
        reset_day: 1,
        last_reset_at: Some(previous_cycle_start_rfc3339.clone()),
        display: Some("GB".to_string()),
    };
    catnap::db::upsert_lazycat_traffic_sample(&t.db, "u_1", &sample)
        .await
        .unwrap();

    let (status, json) = authed_json(&t, "u_1", Method::GET, "/api/lazycat/machines", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(now >= current_cycle_start);
    assert!(json["items"][0]["traffic"].is_null());
    assert_eq!(
        json["items"][0]["lastPanelSyncAt"].as_str(),
        Some(sampled_at_rfc3339.as_str())
    );
}

#[tokio::test]
async fn lazycat_machines_hide_stale_previous_cycle_traffic_without_history_rows() {
    let t = make_app().await;
    let user_id = "u_1";
    let service_id = 9102;
    let primary_address = "vm-charlie.edge.example.test";
    seed_lazycat_machine(
        &t,
        user_id,
        service_id,
        "owner@example.com",
        "旧账期缓存机",
        primary_address,
    )
    .await;

    let now = OffsetDateTime::now_utc();
    let current_cycle_start = PrimitiveDateTime::new(
        Date::from_calendar_date(now.year(), now.month(), 1).unwrap(),
        Time::MIDNIGHT,
    )
    .assume_offset(UtcOffset::UTC);
    let previous_cycle_marker = current_cycle_start - Duration::days(1);
    let previous_cycle_start = PrimitiveDateTime::new(
        Date::from_calendar_date(
            previous_cycle_marker.year(),
            previous_cycle_marker.month(),
            1,
        )
        .unwrap(),
        Time::MIDNIGHT,
    )
    .assume_offset(UtcOffset::UTC);
    let sampled_at = previous_cycle_start + Duration::days(10) + Duration::minutes(20);
    let previous_cycle_start_rfc3339 = previous_cycle_start.format(&Rfc3339).unwrap();
    let sampled_at_rfc3339 = sampled_at.format(&Rfc3339).unwrap();

    let detail = catnap::db::LazycatMachineDetailRecord {
        service_id,
        panel_kind: Some("container".to_string()),
        panel_url: Some(format!(
            "https://panel-{service_id}.example.test:8443/container/dashboard"
        )),
        panel_hash: Some(format!("hash-{service_id}")),
        traffic_used_gb: Some(321.0),
        traffic_limit_gb: Some(800.0),
        traffic_reset_day: Some(1),
        traffic_last_reset_at: Some(previous_cycle_start_rfc3339.clone()),
        traffic_display: Some("321 GB / 800 GB".to_string()),
        detail_state: "stale".to_string(),
        detail_error: Some("panel timeout".to_string()),
        last_panel_sync_at: sampled_at_rfc3339.clone(),
    };
    catnap::db::update_lazycat_machine_detail(&t.db, "u_1", &detail)
        .await
        .unwrap();

    sqlx::query("DELETE FROM lazycat_traffic_samples WHERE user_id = ? AND service_id = ?")
        .bind("u_1")
        .bind(service_id)
        .execute(&t.db)
        .await
        .unwrap();

    let (status, json) = authed_json(&t, "u_1", Method::GET, "/api/lazycat/machines", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(now >= current_cycle_start);
    assert!(json["items"][0]["traffic"].is_null());
    assert_eq!(
        json["items"][0]["lastPanelSyncAt"].as_str(),
        Some(sampled_at_rfc3339.as_str())
    );
}

#[tokio::test]
async fn lazycat_sync_requires_connected_account() {
    let t = make_app().await;
    let (status, json) = authed_json(&t, "u_1", Method::POST, "/api/lazycat/sync", None).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["error"]["code"].as_str(), Some("INVALID_ARGUMENT"));
    assert_eq!(
        json["error"]["message"].as_str(),
        Some("请先连接懒猫云账号")
    );
}

#[tokio::test]
async fn bootstrap_falls_back_to_legacy_telegram_target() {
    let t = make_app().await;
    ensure_user_exists(&t, "u_1").await;
    sqlx::query(
        "UPDATE settings SET telegram_enabled = 1, telegram_bot_token = ?, telegram_target = ?, telegram_targets_json = NULL WHERE user_id = ?",
    )
    .bind("token")
    .bind("@legacy_target")
    .bind("u_1")
    .execute(&t.db)
    .await
    .unwrap();

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
    assert_eq!(
        json["settings"]["notifications"]["telegram"]["targets"][0].as_str(),
        Some("@legacy_target")
    );
    assert_eq!(
        json["settings"]["notifications"]["telegram"]["configured"].as_bool(),
        Some(true)
    );
}

#[tokio::test]
async fn put_settings_ignores_catalog_refresh_overrides() {
    let t = make_app().await;
    ensure_user_exists(&t, "u_1").await;

    sqlx::query("UPDATE settings SET catalog_refresh_auto_interval_hours = ? WHERE user_id = ?")
        .bind(48_i64)
        .bind("u_1")
        .execute(&t.db)
        .await
        .unwrap();

    let bytes = serde_json::to_vec(&serde_json::json!({
        "poll": { "intervalMinutes": 1, "jitterPct": 0.1 },
        "siteBaseUrl": null,
        "catalogRefresh": { "autoIntervalHours": 1 },
        "monitoringEvents": {
            "partitionCatalogChangeEnabled": false,
            "regionPartitionChangeEnabled": false,
            "siteRegionChangeEnabled": false
        },
        "notifications": {
            "telegram": { "enabled": false, "botToken": null, "targets": [] },
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

    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        json["catalogRefresh"]["autoIntervalHours"].as_i64(),
        Some(catnap::defaults::FIXED_CATALOG_TOPOLOGY_REFRESH_INTERVAL_HOURS)
    );

    let row =
        sqlx::query("SELECT catalog_refresh_auto_interval_hours FROM settings WHERE user_id = ?")
            .bind("u_1")
            .fetch_one(&t.db)
            .await
            .unwrap();
    assert_eq!(row.get::<Option<i64>, _>(0), Some(48));
}

#[tokio::test]
async fn products_exposes_optional_source_pid() {
    let t = make_app().await;
    sqlx::query("UPDATE catalog_configs SET source_pid = NULL WHERE id = ?")
        .bind("lc:7:40:127")
        .execute(&t.db)
        .await
        .unwrap();

    let res = t
        .app
        .oneshot(
            Request::builder()
                .uri("/api/products")
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
    let configs = json
        .get("configs")
        .and_then(|v| v.as_array())
        .expect("configs array");

    let with_pid = configs
        .iter()
        .find(|cfg| cfg.get("id").and_then(|v| v.as_str()) == Some("lc:7:40:128"))
        .expect("config with pid exists");
    assert_eq!(
        with_pid.get("sourcePid").and_then(|v| v.as_str()),
        Some("128")
    );
    assert_eq!(
        with_pid.get("sourceFid").and_then(|v| v.as_str()),
        Some("7")
    );
    assert_eq!(
        with_pid.get("sourceGid").and_then(|v| v.as_str()),
        Some("40")
    );

    let without_pid = configs
        .iter()
        .find(|cfg| cfg.get("id").and_then(|v| v.as_str()) == Some("lc:7:40:127"))
        .expect("config without pid exists");
    assert!(without_pid.get("sourcePid").is_none());
    assert_eq!(
        without_pid.get("sourceFid").and_then(|v| v.as_str()),
        Some("7")
    );
    assert_eq!(
        without_pid.get("sourceGid").and_then(|v| v.as_str()),
        Some("40")
    );
}

#[tokio::test]
async fn archive_delisted_products_is_per_user_and_idempotent() {
    let t = make_app().await;
    let delisted_id = "lc:7:40:127";
    let delisted_at = "2026-03-03T08:00:00Z";

    sqlx::query(
        "UPDATE catalog_configs SET lifecycle_state = 'delisted', lifecycle_delisted_at = ? WHERE id = ?",
    )
    .bind(delisted_at)
    .bind(delisted_id)
    .execute(&t.db)
    .await
    .unwrap();

    let res = t
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/products/archive/delisted")
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
    assert_eq!(json["archivedCount"].as_i64(), Some(1));
    assert_eq!(json["archivedIds"].as_array().map(Vec::len), Some(1));
    assert!(json["archivedAt"].as_str().is_some());

    let res = t
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/products/archive/delisted")
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
    assert_eq!(json["archivedCount"].as_i64(), Some(0));
    assert_eq!(json["archivedIds"].as_array().map(Vec::len), Some(0));
    assert!(json.get("archivedAt").is_none());

    let res = t
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/products")
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
    let cfg = json["configs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"].as_str() == Some(delisted_id))
        .unwrap();
    assert!(cfg["lifecycle"]["cleanupAt"].as_str().is_some());

    let res = t
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/products")
                .header("host", "example.com")
                .header("x-user", "u_2")
                .header("origin", "http://example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let cfg = json["configs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"].as_str() == Some(delisted_id))
        .unwrap();
    assert!(cfg["lifecycle"].get("cleanupAt").is_none());
}

#[tokio::test]
async fn relisted_config_clears_archive_cleanup_at() {
    let t = make_app().await;
    let relisted_id = "lc:7:40:127";

    sqlx::query(
        "UPDATE catalog_configs SET lifecycle_state = 'delisted', lifecycle_delisted_at = ? WHERE id = ?",
    )
    .bind("2026-03-03T08:00:00Z")
    .bind(relisted_id)
    .execute(&t.db)
    .await
    .unwrap();

    let res = t
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/products/archive/delisted")
                .header("host", "example.com")
                .header("x-user", "u_1")
                .header("origin", "http://example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let relisted_cfg =
        catnap::upstream::parse_configs("7", Some("40"), include_str!("fixtures/cart-fid-7.html"))
            .into_iter()
            .find(|cfg| cfg.id == relisted_id)
            .unwrap();
    catnap::db::upsert_catalog_configs(&t.db, &[relisted_cfg])
        .await
        .unwrap();

    let res = t
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/products")
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
    let cfg = json["configs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"].as_str() == Some(relisted_id))
        .unwrap();
    assert_eq!(cfg["lifecycle"]["state"].as_str(), Some("active"));
    assert!(cfg["lifecycle"].get("cleanupAt").is_none());
}

#[tokio::test]
async fn about_returns_repo_and_version() {
    let t = make_app().await;
    let res = t
        .app
        .oneshot(
            Request::builder()
                .uri("/api/about")
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
    assert_eq!(json.get("version").and_then(|v| v.as_str()), Some("test"));
    assert!(json.get("repoUrl").is_some());
    assert_eq!(
        json.pointer("/update/enabled").and_then(|v| v.as_bool()),
        Some(false)
    );
    assert_eq!(
        json.pointer("/update/status").and_then(|v| v.as_str()),
        Some("disabled")
    );
}

#[tokio::test]
async fn about_can_force_refresh_update_cache() {
    let gh = axum::Router::new().route(
        "/repos/example/repo/releases/latest",
        axum::routing::get(|| async {
            axum::Json(serde_json::json!({
                "tag_name": "v9.9.9",
                "html_url": "https://example.invalid/releases/tag/v9.9.9",
            }))
        }),
    );
    let gh_base = spawn_stub_server(gh).await;

    let mut cfg = test_config();
    cfg.effective_version = "0.1.0".to_string();
    cfg.update_check_enabled = true;
    cfg.update_check_ttl_seconds = 3600;
    cfg.update_repo = "example/repo".to_string();
    cfg.github_api_base_url = gh_base;

    let t = make_app_with_config(cfg).await;
    let res = t
        .app
        .oneshot(
            Request::builder()
                .uri("/api/about?force=1")
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
    assert_eq!(json.get("version").and_then(|v| v.as_str()), Some("0.1.0"));
    assert_eq!(
        json.pointer("/update/enabled").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        json.pointer("/update/status").and_then(|v| v.as_str()),
        Some("ok")
    );
    assert!(json
        .pointer("/update/checkedAt")
        .and_then(|v| v.as_str())
        .is_some());
    assert_eq!(
        json.pointer("/update/latestVersion")
            .and_then(|v| v.as_str()),
        Some("9.9.9")
    );
    assert_eq!(
        json.pointer("/update/latestUrl").and_then(|v| v.as_str()),
        Some("https://example.invalid/releases/tag/v9.9.9")
    );
    assert_eq!(
        json.pointer("/update/updateAvailable")
            .and_then(|v| v.as_bool()),
        Some(true)
    );
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
async fn monitoring_partition_toggle_persists_in_bootstrap_without_touching_card_monitors() {
    let t = make_app().await;

    let bootstrap_before = t
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
    assert_eq!(bootstrap_before.status(), StatusCode::OK);
    let bytes = to_bytes(bootstrap_before.into_body(), usize::MAX)
        .await
        .unwrap();
    let before_json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let before_card_monitor = before_json["catalog"]["configs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|cfg| cfg["id"].as_str() == Some("lc:7:40:128"))
        .and_then(|cfg| cfg["monitorEnabled"].as_bool());
    assert_eq!(before_card_monitor, Some(false));

    let toggle_bytes = serde_json::to_vec(&serde_json::json!({
        "countryId": "7",
        "regionId": "40",
        "enabled": true
    }))
    .unwrap();
    let toggle_res = t
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/monitoring/partitions")
                .header("host", "example.com")
                .header("x-user", "u_1")
                .header("origin", "http://example.com")
                .header("content-type", "application/json")
                .body(Body::from(toggle_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(toggle_res.status(), StatusCode::OK);

    let bootstrap_after = t
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
    assert_eq!(bootstrap_after.status(), StatusCode::OK);
    let bytes = to_bytes(bootstrap_after.into_body(), usize::MAX)
        .await
        .unwrap();
    let after_json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let enabled_partition = after_json["monitoring"]["enabledPartitions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|partition| {
            partition["countryId"].as_str() == Some("7")
                && partition["regionId"].as_str() == Some("40")
        });
    assert!(enabled_partition);

    let after_card_monitor = after_json["catalog"]["configs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|cfg| cfg["id"].as_str() == Some("lc:7:40:128"))
        .and_then(|cfg| cfg["monitorEnabled"].as_bool());
    assert_eq!(after_card_monitor, Some(false));
}

#[tokio::test]
async fn monitoring_partition_toggle_rejects_unknown_partition() {
    let t = make_app().await;
    let bytes = serde_json::to_vec(&serde_json::json!({
        "countryId": "7",
        "regionId": "999",
        "enabled": true
    }))
    .unwrap();

    let res = t
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/monitoring/partitions")
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
async fn monitoring_partition_toggle_rejects_removed_country_scope() {
    let t = make_app().await;
    catnap::db::replace_catalog_topology(&t.db, "https://lxc.lazycat.wiki/cart", &[], &[])
        .await
        .unwrap();

    let bytes = serde_json::to_vec(&serde_json::json!({
        "countryId": "7",
        "regionId": null,
        "enabled": true
    }))
    .unwrap();

    let res = t
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/monitoring/partitions")
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
async fn monitoring_partition_toggle_accepts_country_scope_with_null_region() {
    let t = make_app().await;
    let bytes = serde_json::to_vec(&serde_json::json!({
        "countryId": "7",
        "regionId": null,
        "enabled": true
    }))
    .unwrap();

    let res = t
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/monitoring/partitions")
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

    let row = sqlx::query(
        "SELECT country_id, region_id, enabled FROM monitoring_partitions WHERE user_id = ? AND partition_key = ?",
    )
    .bind("u_1")
    .bind("7::")
    .fetch_one(&t.db)
    .await
    .unwrap();
    assert_eq!(row.get::<String, _>(0), "7");
    assert_eq!(row.get::<Option<String>, _>(1), None);
    assert_eq!(row.get::<i64, _>(2), 1);
}

#[tokio::test]
async fn settings_migrates_legacy_monitoring_flags_into_new_hierarchy() {
    let cfg = test_config();
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&cfg.db_url)
        .await
        .unwrap();

    sqlx::query(
        r#"
CREATE TABLE settings (
  user_id TEXT PRIMARY KEY,
  poll_interval_minutes INTEGER NOT NULL,
  poll_jitter_pct REAL NOT NULL,
  site_base_url TEXT NULL,
  monitoring_events_listed_enabled INTEGER NOT NULL DEFAULT 0,
  monitoring_events_delisted_enabled INTEGER NOT NULL DEFAULT 0,
  telegram_enabled INTEGER NOT NULL,
  telegram_bot_token TEXT NULL,
  telegram_target TEXT NULL,
  web_push_enabled INTEGER NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
)
"#,
    )
    .execute(&db)
    .await
    .unwrap();

    sqlx::query(
        r#"
INSERT INTO settings (
  user_id,
  poll_interval_minutes,
  poll_jitter_pct,
  site_base_url,
  monitoring_events_listed_enabled,
  monitoring_events_delisted_enabled,
  telegram_enabled,
  telegram_bot_token,
  telegram_target,
  web_push_enabled,
  created_at,
  updated_at
)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
"#,
    )
    .bind("u_legacy")
    .bind(5_i64)
    .bind(0.2_f64)
    .bind(Option::<String>::None)
    .bind(1_i64)
    .bind(1_i64)
    .bind(0_i64)
    .bind(Option::<String>::None)
    .bind(Option::<String>::None)
    .bind(0_i64)
    .bind("2026-03-10T00:00:00Z")
    .bind("2026-03-10T00:00:00Z")
    .execute(&db)
    .await
    .unwrap();

    catnap::db::init_db(&db).await.unwrap();

    let migrated = sqlx::query(
        "SELECT monitoring_events_partition_catalog_change_enabled, monitoring_events_region_partition_change_enabled, monitoring_events_site_region_change_enabled, monitoring_events_delisted_enabled FROM settings WHERE user_id = ?",
    )
    .bind("u_legacy")
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(migrated.get::<i64, _>(0), 0);
    assert_eq!(migrated.get::<i64, _>(1), 0);
    assert_eq!(migrated.get::<i64, _>(2), 1);
    assert_eq!(migrated.get::<i64, _>(3), 1);

    let catalog = std::sync::Arc::new(tokio::sync::RwLock::new(
        catnap::upstream::CatalogSnapshot::empty(cfg.upstream_cart_url.clone()),
    ));
    let ops = catnap::ops::OpsManager::new(cfg.clone(), db.clone(), catalog.clone());
    ops.start();
    let state = AppState {
        config: cfg,
        db: db.clone(),
        catalog,
        catalog_refresh: catnap::catalog_refresh::CatalogRefreshManager::new(),
        ops,
        update_cache: catnap::update_check::new_cache(),
        lazycat_sync_users: Arc::new(tokio::sync::Mutex::new(HashSet::new())),
    };
    let app = build_app(state);

    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/settings")
                .header("host", "example.com")
                .header("x-user", "u_legacy")
                .header("origin", "http://example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        json["monitoringEvents"]["partitionCatalogChangeEnabled"].as_bool(),
        Some(false)
    );
    assert_eq!(
        json["monitoringEvents"]["regionPartitionChangeEnabled"].as_bool(),
        Some(false)
    );
    assert_eq!(
        json["monitoringEvents"]["siteRegionChangeEnabled"].as_bool(),
        Some(true)
    );
    assert!(json["monitoringEvents"].get("delistedEnabled").is_none());
}

#[tokio::test]
async fn settings_migration_does_not_fall_back_to_older_listed_flag_when_site_flag_is_off() {
    let cfg = test_config();
    let db = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&cfg.db_url)
        .await
        .unwrap();

    sqlx::query(
        r#"
CREATE TABLE settings (
  user_id TEXT PRIMARY KEY,
  poll_interval_minutes INTEGER NOT NULL,
  poll_jitter_pct REAL NOT NULL,
  site_base_url TEXT NULL,
  monitoring_events_listed_enabled INTEGER NOT NULL DEFAULT 0,
  monitoring_events_partition_listed_enabled INTEGER NOT NULL DEFAULT 0,
  monitoring_events_site_listed_enabled INTEGER NOT NULL DEFAULT 0,
  monitoring_events_delisted_enabled INTEGER NOT NULL DEFAULT 0,
  telegram_enabled INTEGER NOT NULL,
  telegram_bot_token TEXT NULL,
  telegram_target TEXT NULL,
  web_push_enabled INTEGER NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
)
"#,
    )
    .execute(&db)
    .await
    .unwrap();

    sqlx::query(
        r#"
INSERT INTO settings (
  user_id,
  poll_interval_minutes,
  poll_jitter_pct,
  site_base_url,
  monitoring_events_listed_enabled,
  monitoring_events_partition_listed_enabled,
  monitoring_events_site_listed_enabled,
  monitoring_events_delisted_enabled,
  telegram_enabled,
  telegram_bot_token,
  telegram_target,
  web_push_enabled,
  created_at,
  updated_at
)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
"#,
    )
    .bind("u_legacy_site_off")
    .bind(5_i64)
    .bind(0.2_f64)
    .bind(Option::<String>::None)
    .bind(1_i64)
    .bind(0_i64)
    .bind(0_i64)
    .bind(0_i64)
    .bind(0_i64)
    .bind(Option::<String>::None)
    .bind(Option::<String>::None)
    .bind(0_i64)
    .bind("2026-03-10T00:00:00Z")
    .bind("2026-03-10T00:00:00Z")
    .execute(&db)
    .await
    .unwrap();

    catnap::db::init_db(&db).await.unwrap();

    let migrated = sqlx::query(
        "SELECT monitoring_events_site_region_change_enabled FROM settings WHERE user_id = ?",
    )
    .bind("u_legacy_site_off")
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(migrated.get::<i64, _>(0), 0);
}

#[tokio::test]
async fn telegram_test_returns_400_when_missing_token_or_target() {
    let t = make_app().await;
    let (status, json) = post_telegram_test(
        &t,
        "u_1",
        serde_json::json!({
            "botToken": null,
            "targets": null,
            "text": null,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["error"]["code"], "INVALID_ARGUMENT");
}

#[tokio::test]
async fn telegram_test_does_not_fall_back_when_request_targets_are_explicitly_empty() {
    let t = make_app().await;
    ensure_user_exists(&t, "u_1").await;
    sqlx::query(
        "UPDATE settings SET telegram_enabled = 1, telegram_bot_token = ?, telegram_target = ?, telegram_targets_json = ? WHERE user_id = ?",
    )
    .bind("saved-token")
    .bind("@saved-target")
    .bind("[\"@saved-target\"]")
    .bind("u_1")
    .execute(&t.db)
    .await
    .unwrap();

    let (status, json) = post_telegram_test(
        &t,
        "u_1",
        serde_json::json!({
            "botToken": "request-token",
            "targets": [],
            "text": null,
        }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["error"]["code"], "INVALID_ARGUMENT");
    assert_eq!(
        json["error"]["message"].as_str(),
        Some("缺少 targets（可在本次请求提供或先在设置中保存）")
    );
}

#[tokio::test]
async fn telegram_test_returns_5xx_when_upstream_fails() {
    let tg = axum::Router::new().route(
        "/*path",
        axum::routing::post(|| async {
            (
                StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({
                    "ok": false,
                    "error_code": 400,
                    "description": "Bad Request: chat not found"
                })),
            )
        }),
    );
    let base = spawn_stub_server(tg).await;

    let mut cfg = test_config();
    cfg.telegram_api_base_url = base;
    let t = make_app_with_config(cfg).await;

    ensure_user_exists(&t, "u_1").await;
    save_telegram_settings(&t, "u_1", "t", "@c").await;

    let (status, json) = post_telegram_test(
        &t,
        "u_1",
        serde_json::json!({ "botToken": null, "targets": null, "text": "hi" }),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let msg = json["results"][0]["error"].as_str().unwrap_or_default();
    assert!(msg.contains("chat not found"));
}

#[tokio::test]
async fn telegram_test_uses_friendly_default_text() {
    let captured = Arc::new(Mutex::new(None::<serde_json::Value>));
    let captured2 = captured.clone();
    let tg = axum::Router::new().route(
        "/*path",
        axum::routing::post(move |body: axum::Json<serde_json::Value>| {
            let captured = captured2.clone();
            async move {
                *captured.lock().unwrap() = Some(body.0);
                StatusCode::OK
            }
        }),
    );
    let base = spawn_stub_server(tg).await;

    let mut cfg = test_config();
    cfg.telegram_api_base_url = base;
    let t = make_app_with_config(cfg).await;

    ensure_user_exists(&t, "u_1").await;
    save_telegram_settings(&t, "u_1", "t", "@c").await;

    let (status, json) = post_telegram_test(
        &t,
        "u_1",
        serde_json::json!({ "botToken": null, "targets": null, "text": null }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["ok"], true);

    let payload = captured
        .lock()
        .unwrap()
        .clone()
        .expect("telegram request captured");
    let text = payload["text"].as_str().expect("telegram text payload");
    assert!(text.starts_with(
        "【Telegram 测试】通知配置正常
如果你看到这条消息，说明 Catnap 已可发送 Telegram 通知。
时间："
    ));
    assert!(!text.contains("user="));
    assert!(!text.contains("catnap 测试消息"));
}

#[tokio::test]
async fn telegram_test_returns_partial_success_for_multi_targets() {
    let tg = axum::Router::new().route(
        "/*path",
        axum::routing::post(|body: axum::Json<serde_json::Value>| async move {
            let chat_id = body["chat_id"].as_str().unwrap_or_default();
            if chat_id == "@ok" {
                return (
                    StatusCode::OK,
                    axum::Json(serde_json::json!({ "ok": true, "result": { "chat_id": chat_id } })),
                );
            }
            (
                StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({
                    "ok": false,
                    "error_code": 400,
                    "description": "Bad Request: chat not found"
                })),
            )
        }),
    );
    let base = spawn_stub_server(tg).await;

    let mut cfg = test_config();
    cfg.telegram_api_base_url = base;
    let t = make_app_with_config(cfg).await;

    let (status, json) = post_telegram_test(
        &t,
        "u_1",
        serde_json::json!({
            "botToken": "t",
            "targets": ["@ok", "@bad"],
            "text": "hi",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"].as_str(), Some("partial_success"));
    let results = json["results"].as_array().unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["target"].as_str(), Some("@ok"));
    assert_eq!(results[0]["status"].as_str(), Some("success"));
    assert_eq!(results[1]["target"].as_str(), Some("@bad"));
    assert_eq!(results[1]["status"].as_str(), Some("error"));
    assert!(results[1]["error"]
        .as_str()
        .unwrap_or_default()
        .contains("chat not found"));
}

#[tokio::test]
async fn telegram_test_surfaces_migrate_to_chat_id_hint() {
    let tg = axum::Router::new().route(
        "/*path",
        axum::routing::post(|| async {
            (
                StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({
                    "ok": false,
                    "error_code": 400,
                    "description": "Bad Request: group chat was upgraded to a supergroup chat",
                    "parameters": { "migrate_to_chat_id": -1002233445566_i64 }
                })),
            )
        }),
    );
    let base = spawn_stub_server(tg).await;

    let mut cfg = test_config();
    cfg.telegram_api_base_url = base;
    let t = make_app_with_config(cfg).await;

    ensure_user_exists(&t, "u_1").await;
    save_telegram_settings(&t, "u_1", "t", "-12345").await;

    let (status, json) = post_telegram_test(
        &t,
        "u_1",
        serde_json::json!({ "botToken": null, "targets": null, "text": "hi" }),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let msg = json["results"][0]["error"].as_str().unwrap_or_default();
    assert!(msg.contains("migrate_to_chat_id=-1002233445566"));
}

#[tokio::test]
async fn telegram_test_surfaces_retry_after_hint() {
    let tg = axum::Router::new().route(
        "/*path",
        axum::routing::post(|| async {
            (
                StatusCode::TOO_MANY_REQUESTS,
                axum::Json(serde_json::json!({
                    "ok": false,
                    "error_code": 429,
                    "description": "Too Many Requests: retry later",
                    "parameters": { "retry_after": 17 }
                })),
            )
        }),
    );
    let base = spawn_stub_server(tg).await;

    let mut cfg = test_config();
    cfg.telegram_api_base_url = base;
    let t = make_app_with_config(cfg).await;

    ensure_user_exists(&t, "u_1").await;
    save_telegram_settings(&t, "u_1", "t", "@c").await;

    let (status, json) = post_telegram_test(
        &t,
        "u_1",
        serde_json::json!({ "botToken": null, "targets": null, "text": "hi" }),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let msg = json["results"][0]["error"].as_str().unwrap_or_default();
    assert!(msg.contains("retry_after=17s"));
}

#[tokio::test]
async fn telegram_test_surfaces_plain_text_error_body() {
    let tg = axum::Router::new().route(
        "/*path",
        axum::routing::post(|| async {
            (
                StatusCode::BAD_REQUEST,
                "Bad Request:\nchat not found\r\nPlease check bot permission.",
            )
        }),
    );
    let base = spawn_stub_server(tg).await;

    let mut cfg = test_config();
    cfg.telegram_api_base_url = base;
    let t = make_app_with_config(cfg).await;

    ensure_user_exists(&t, "u_1").await;
    save_telegram_settings(&t, "u_1", "t", "@c").await;

    let (status, json) = post_telegram_test(
        &t,
        "u_1",
        serde_json::json!({ "botToken": null, "targets": null, "text": "hi" }),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let msg = json["results"][0]["error"].as_str().unwrap_or_default();
    assert!(msg.contains("upstream returned non-json error body"));
}

#[tokio::test]
async fn telegram_test_marks_truncated_upstream_body() {
    let body = "X".repeat(20_000);
    let tg = axum::Router::new().route(
        "/*path",
        axum::routing::post(move || {
            let body = body.clone();
            async move { (StatusCode::BAD_REQUEST, body) }
        }),
    );
    let base = spawn_stub_server(tg).await;

    let mut cfg = test_config();
    cfg.telegram_api_base_url = base;
    let t = make_app_with_config(cfg).await;

    ensure_user_exists(&t, "u_1").await;
    save_telegram_settings(&t, "u_1", "t", "@c").await;

    let (status, json) = post_telegram_test(
        &t,
        "u_1",
        serde_json::json!({ "botToken": null, "targets": null, "text": "hi" }),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let msg = json["results"][0]["error"].as_str().unwrap_or_default();
    assert!(msg.contains("upstream_body_truncated"));
}

#[tokio::test]
async fn telegram_test_redacts_token_from_upstream_description() {
    let token = "123456:abcDEF_sensitive";
    let token_with_newline = "123456:abc\nDEF_sensitive";
    let tg = axum::Router::new().route(
        "/*path",
        axum::routing::post(move || async move {
            (
                StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({
                    "ok": false,
                    "error_code": 400,
                    "description": format!(
                        "Bad Request: bot{token_with_newline}/sendMessage rejected token={token_with_newline}"
                    ),
                })),
            )
        }),
    );
    let base = spawn_stub_server(tg).await;

    let mut cfg = test_config();
    cfg.telegram_api_base_url = base;
    let t = make_app_with_config(cfg).await;

    ensure_user_exists(&t, "u_1").await;
    save_telegram_settings(&t, "u_1", token, "@c").await;

    let (status, json) = post_telegram_test(
        &t,
        "u_1",
        serde_json::json!({ "botToken": null, "targets": null, "text": "hi" }),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let msg = json["results"][0]["error"].as_str().unwrap_or_default();
    assert!(!msg.contains(token));
    assert!(msg.contains("[REDACTED]"));
}

#[tokio::test]
async fn telegram_test_redacts_token_with_newline_after_bot_prefix() {
    let token = "123456:abcDEF_sensitive";
    let tg = axum::Router::new().route(
        "/*path",
        axum::routing::post(move || async move {
            (
                StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({
                    "ok": false,
                    "error_code": 400,
                    "description": format!("Bad Request: bot\n{token}/sendMessage rejected"),
                })),
            )
        }),
    );
    let base = spawn_stub_server(tg).await;

    let mut cfg = test_config();
    cfg.telegram_api_base_url = base;
    let t = make_app_with_config(cfg).await;

    ensure_user_exists(&t, "u_1").await;
    save_telegram_settings(&t, "u_1", token, "@c").await;

    let (status, json) = post_telegram_test(
        &t,
        "u_1",
        serde_json::json!({ "botToken": null, "targets": null, "text": "hi" }),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let msg = json["results"][0]["error"].as_str().unwrap_or_default();
    assert!(!msg.contains(token));
    assert!(msg.contains("bot [REDACTED]/sendMessage"));
}

#[tokio::test]
async fn telegram_test_redacts_url_encoded_token() {
    let token = "123456:abcDEF_sensitive";
    let token_encoded = token.replace(':', "%3a");
    let token_encoded_for_resp = token_encoded.clone();
    let tg = axum::Router::new().route(
        "/*path",
        axum::routing::post(move || {
            let token_encoded = token_encoded_for_resp.clone();
            async move {
                (
                    StatusCode::BAD_REQUEST,
                    axum::Json(serde_json::json!({
                        "ok": false,
                        "error_code": 400,
                        "description": format!(
                            "Bad Request: bot{token_encoded}/sendMessage rejected token={token_encoded}"
                        ),
                    })),
                )
            }
        }),
    );
    let base = spawn_stub_server(tg).await;

    let mut cfg = test_config();
    cfg.telegram_api_base_url = base;
    let t = make_app_with_config(cfg).await;

    ensure_user_exists(&t, "u_1").await;
    save_telegram_settings(&t, "u_1", token, "@c").await;

    let (status, json) = post_telegram_test(
        &t,
        "u_1",
        serde_json::json!({ "botToken": null, "targets": null, "text": "hi" }),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let msg = json["results"][0]["error"].as_str().unwrap_or_default();
    assert!(!msg.contains(token));
    assert!(!msg.contains(&token_encoded));
    assert!(msg.contains("bot[REDACTED]/sendMessage"));
    assert!(msg.contains("token=[REDACTED]"));
}

#[tokio::test]
async fn telegram_test_redacts_token_with_uppercase_bot_prefix() {
    let token = "123456:abcDEF_sensitive";
    let tg = axum::Router::new().route(
        "/*path",
        axum::routing::post(move || async move {
            (
                StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({
                    "ok": false,
                    "error_code": 400,
                    "description": format!("Bad Request: BOT{token}/sendMessage rejected"),
                })),
            )
        }),
    );
    let base = spawn_stub_server(tg).await;

    let mut cfg = test_config();
    cfg.telegram_api_base_url = base;
    let t = make_app_with_config(cfg).await;

    ensure_user_exists(&t, "u_1").await;
    save_telegram_settings(&t, "u_1", token, "@c").await;

    let (status, json) = post_telegram_test(
        &t,
        "u_1",
        serde_json::json!({ "botToken": null, "targets": null, "text": "hi" }),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let msg = json["results"][0]["error"].as_str().unwrap_or_default();
    assert!(!msg.contains(token));
    assert!(msg.contains("BOT[REDACTED]/sendMessage"));
}

#[tokio::test]
async fn telegram_test_redacts_token_after_whitespace_boundary() {
    let token = "123456:abcDEF_sensitive";
    let tg = axum::Router::new().route(
        "/*path",
        axum::routing::post(move || async move {
            (
                StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({
                    "ok": false,
                    "error_code": 400,
                    "description": format!("Bad Request: invalid token {token}"),
                })),
            )
        }),
    );
    let base = spawn_stub_server(tg).await;

    let mut cfg = test_config();
    cfg.telegram_api_base_url = base;
    let t = make_app_with_config(cfg).await;

    ensure_user_exists(&t, "u_1").await;
    save_telegram_settings(&t, "u_1", token, "@c").await;

    let (status, json) = post_telegram_test(
        &t,
        "u_1",
        serde_json::json!({ "botToken": null, "targets": null, "text": "hi" }),
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    let msg = json["results"][0]["error"].as_str().unwrap_or_default();
    assert!(!msg.contains(token));
    assert!(msg.contains("token [REDACTED]"));
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

    let bytes = serde_json::to_vec(&serde_json::json!({})).unwrap();

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

#[tokio::test]
async fn notification_records_api_lists_and_fetches_detail() {
    let t = make_app().await;
    ensure_user_exists(&t, "u_1").await;

    let config_id = sqlx::query("SELECT id FROM catalog_configs ORDER BY id LIMIT 1")
        .fetch_one(&t.db)
        .await
        .unwrap()
        .get::<String, _>(0);
    let item = catnap::db::load_notification_record_item_snapshot(&t.db, &config_id)
        .await
        .unwrap()
        .unwrap();

    let older_id = catnap::db::insert_notification_record(
        &t.db,
        "u_1",
        &catnap::models::NotificationRecordDraft {
            kind: "monitoring.config".to_string(),
            title: "【配置更新】Older".to_string(),
            summary: "库存 1｜¥1.00 / 月".to_string(),
            partition_label: item.partition_label.clone(),
            telegram_status: "success".to_string(),
            web_push_status: "skipped".to_string(),
            items: vec![item.clone()],
        },
    )
    .await
    .unwrap();
    let newer_id = catnap::db::insert_notification_record(
        &t.db,
        "u_1",
        &catnap::models::NotificationRecordDraft {
            kind: "catalog.delisted".to_string(),
            title: "【已下架】Newer".to_string(),
            summary: "最近状态：库存 1｜¥1.00 / 月".to_string(),
            partition_label: item.partition_label.clone(),
            telegram_status: "error".to_string(),
            web_push_status: "skipped".to_string(),
            items: vec![item.clone()],
        },
    )
    .await
    .unwrap();

    sqlx::query("UPDATE notification_records SET created_at = ? WHERE id = ?")
        .bind("2026-03-11T10:05:00.000000000Z")
        .bind(&older_id)
        .execute(&t.db)
        .await
        .unwrap();
    sqlx::query("UPDATE notification_records SET created_at = ? WHERE id = ?")
        .bind("2026-03-11T10:05:00.100000000Z")
        .bind(&newer_id)
        .execute(&t.db)
        .await
        .unwrap();

    let res = t
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/notifications/records?limit=1")
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
    assert_eq!(items[0]["id"].as_str(), Some(newer_id.as_str()));
    assert_eq!(items[0]["telegramStatus"].as_str(), Some("error"));
    assert_eq!(
        items[0]["items"][0]["configId"].as_str(),
        Some(config_id.as_str())
    );
    assert!(items[0]["items"][0]["partitionLabel"].as_str().is_some());
    assert_eq!(
        json["nextCursor"].as_str(),
        Some(format!("2026-03-11T10:05:00.100000000Z:{newer_id}").as_str())
    );

    let res = t
        .app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/notifications/records?limit=1&cursor=2026-03-11T10:05:00.1%2B00:00:{newer_id}"))
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
    assert_eq!(json["items"][0]["id"].as_str(), Some(older_id.as_str()));
    assert!(json["nextCursor"].is_null());

    let res = t
        .app
        .oneshot(
            Request::builder()
                .uri(format!("/api/notifications/records/{newer_id}"))
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
    assert_eq!(json["id"].as_str(), Some(newer_id.as_str()));
    assert_eq!(json["items"][0]["name"].as_str(), Some(item.name.as_str()));
}

#[tokio::test]
async fn notification_records_api_includes_telegram_deliveries() {
    let t = make_app().await;
    ensure_user_exists(&t, "u_1").await;

    let config_id = sqlx::query("SELECT id FROM catalog_configs ORDER BY id LIMIT 1")
        .fetch_one(&t.db)
        .await
        .unwrap()
        .get::<String, _>(0);
    let item = catnap::db::load_notification_record_item_snapshot(&t.db, &config_id)
        .await
        .unwrap()
        .unwrap();

    let record_id = catnap::db::insert_notification_record(
        &t.db,
        "u_1",
        &catnap::models::NotificationRecordDraft {
            kind: "monitoring.config".to_string(),
            title: "【配置更新】Partial".to_string(),
            summary: "库存 1｜¥1.00 / 月".to_string(),
            partition_label: item.partition_label.clone(),
            telegram_status: "pending".to_string(),
            web_push_status: "skipped".to_string(),
            items: vec![item.clone()],
        },
    )
    .await
    .unwrap();
    catnap::db::replace_notification_record_deliveries(
        &t.db,
        &record_id,
        "telegram",
        &[
            catnap::models::NotificationRecordDeliveryView {
                channel: "telegram".to_string(),
                target: "@ok".to_string(),
                status: "success".to_string(),
                error: None,
            },
            catnap::models::NotificationRecordDeliveryView {
                channel: "telegram".to_string(),
                target: "@bad".to_string(),
                status: "error".to_string(),
                error: Some("telegram http 400: chat not found".to_string()),
            },
        ],
    )
    .await
    .unwrap();
    sqlx::query(
        r#"
INSERT INTO notification_record_deliveries (
  id,
  record_id,
  channel,
  position,
  target,
  status,
  error_message,
  created_at,
  updated_at
)
VALUES (?, ?, 'webPush', 0, ?, 'success', NULL, ?, ?)
"#,
    )
    .bind("delivery-web-push")
    .bind(&record_id)
    .bind("https://push.example.com/subscriptions/demo")
    .bind("2026-03-14T00:00:00Z")
    .bind("2026-03-14T00:00:00Z")
    .execute(&t.db)
    .await
    .unwrap();
    catnap::db::update_notification_record_channel_status(
        &t.db,
        &record_id,
        "telegram",
        "partial_success",
    )
    .await
    .unwrap();

    let res = t
        .app
        .oneshot(
            Request::builder()
                .uri(format!("/api/notifications/records/{record_id}"))
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
    assert_eq!(json["telegramStatus"].as_str(), Some("partial_success"));
    let deliveries = json["telegramDeliveries"].as_array().unwrap();
    assert_eq!(deliveries.len(), 2);
    assert_eq!(deliveries[0]["target"].as_str(), Some("@ok"));
    assert_eq!(deliveries[1]["target"].as_str(), Some("@bad"));
    let by_target = deliveries
        .iter()
        .map(|item| {
            (
                item["target"].as_str().unwrap_or_default().to_string(),
                (
                    item["status"].as_str().unwrap_or_default().to_string(),
                    item["error"].as_str().map(str::to_string),
                ),
            )
        })
        .collect::<std::collections::HashMap<_, _>>();
    assert_eq!(
        by_target.get("@ok").map(|(status, _)| status.as_str()),
        Some("success")
    );
    assert_eq!(
        by_target.get("@bad").map(|(status, _)| status.as_str()),
        Some("error")
    );
    assert!(by_target
        .get("@bad")
        .and_then(|(_, error)| error.as_deref())
        .unwrap_or_default()
        .contains("chat not found"));
}

#[tokio::test]
async fn notification_records_api_rejects_invalid_cursor() {
    let t = make_app().await;
    let res = t
        .app
        .oneshot(
            Request::builder()
                .uri("/api/notifications/records?cursor=bad-cursor")
                .header("host", "example.com")
                .header("x-user", "u_1")
                .header("origin", "http://example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["error"]["code"].as_str(), Some("INVALID_ARGUMENT"));
    assert_eq!(
        json["error"]["message"].as_str(),
        Some("cursor 必须是 <RFC3339>:<id>")
    );
}

#[tokio::test]
async fn notification_record_detail_returns_404_when_missing() {
    let t = make_app().await;
    let res = t
        .app
        .oneshot(
            Request::builder()
                .uri("/api/notifications/records/nr_missing")
                .header("host", "example.com")
                .header("x-user", "u_1")
                .header("origin", "http://example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);

    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["error"]["code"].as_str(), Some("NOT_FOUND"));
    assert_eq!(
        json["error"]["message"].as_str(),
        Some("记录不存在或已过期")
    );
}
