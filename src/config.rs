use std::env;

#[derive(Clone, Debug)]
pub struct RuntimeConfig {
    pub bind_addr: String,
    pub effective_version: String,
    pub repo_url: String,
    pub github_api_base_url: String,
    pub update_check_repo: String,
    pub update_check_enabled: bool,
    pub update_check_ttl_seconds: i64,

    pub upstream_cart_url: String,

    /// Base URL for Telegram Bot API. Used to allow local stubs in tests.
    pub telegram_api_base_url: String,

    /// Name of the request header (provided by a trusted reverse proxy) used to identify the user.
    pub auth_user_header: Option<String>,

    /// Local dev escape hatch: when set, missing/empty user header is treated as this user id.
    /// Intended for local UI review without a reverse proxy; keep unset in real deployments.
    pub dev_user_id: Option<String>,

    pub default_poll_interval_minutes: i64,
    pub default_poll_jitter_pct: f64,

    pub log_retention_days: i64,
    pub log_retention_max_rows: i64,

    // Ops / observability
    pub ops_worker_concurrency: usize,
    pub ops_sse_replay_window_seconds: i64,
    pub ops_log_retention_days: i64,
    pub ops_log_tail_limit_default: i64,
    pub ops_queue_task_limit_default: i64,

    pub db_url: String,

    pub web_push_vapid_public_key: Option<String>,
    pub web_push_vapid_private_key: Option<String>,
    pub web_push_vapid_subject: Option<String>,

    /// Test-only escape hatch for integration tests (never enabled via env).
    pub allow_insecure_local_web_push_endpoints: bool,
}

impl RuntimeConfig {
    pub fn from_env() -> Self {
        let effective_version = env::var("APP_EFFECTIVE_VERSION")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string());

        let repo_url = env::var("CATNAP_REPO_URL")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "https://github.com/IvanLi-CN/catnap".to_string());

        let github_api_base_url = env::var("CATNAP_GITHUB_API_BASE_URL")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "https://api.github.com".to_string());

        let update_check_repo = env::var("CATNAP_UPDATE_CHECK_REPO")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "IvanLi-CN/catnap".to_string());

        let update_check_enabled = env::var("CATNAP_UPDATE_CHECK_ENABLED")
            .ok()
            .and_then(|v| parse_bool(&v))
            .unwrap_or(true);

        let update_check_ttl_seconds = env::var("CATNAP_UPDATE_CHECK_TTL_SECONDS")
            .ok()
            .and_then(|v| v.trim().parse::<i64>().ok())
            .filter(|v| *v >= 0)
            .unwrap_or(3600);

        let telegram_api_base_url = env::var("CATNAP_TELEGRAM_API_BASE_URL")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "https://api.telegram.org".to_string());

        let auth_user_header = env::var("CATNAP_AUTH_USER_HEADER")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());

        let dev_user_id = env::var("CATNAP_DEV_USER_ID")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());

        let default_poll_interval_minutes = env::var("CATNAP_DEFAULT_POLL_INTERVAL_MINUTES")
            .ok()
            .and_then(|v| v.trim().parse::<i64>().ok())
            .filter(|v| *v >= 1)
            .unwrap_or(1);

        let default_poll_jitter_pct = env::var("CATNAP_DEFAULT_POLL_JITTER_PCT")
            .ok()
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|v| (0.0..=1.0).contains(v))
            .unwrap_or(0.1);

        let log_retention_days = env::var("CATNAP_LOG_RETENTION_DAYS")
            .ok()
            .and_then(|v| v.trim().parse::<i64>().ok())
            .filter(|v| *v >= 0)
            .unwrap_or(7);

        let log_retention_max_rows = env::var("CATNAP_LOG_RETENTION_MAX_ROWS")
            .ok()
            .and_then(|v| v.trim().parse::<i64>().ok())
            .filter(|v| *v >= 0)
            .unwrap_or(10_000);

        let ops_worker_concurrency = env::var("CATNAP_OPS_WORKER_CONCURRENCY")
            .ok()
            .and_then(|v| v.trim().parse::<i64>().ok())
            .filter(|v| (1..=64).contains(v))
            .unwrap_or(2) as usize;

        let ops_sse_replay_window_seconds = env::var("CATNAP_OPS_SSE_REPLAY_WINDOW_SECONDS")
            .ok()
            .and_then(|v| v.trim().parse::<i64>().ok())
            .filter(|v| *v >= 1)
            .unwrap_or(3600);

        let ops_log_retention_days = env::var("CATNAP_OPS_LOG_RETENTION_DAYS")
            .ok()
            .and_then(|v| v.trim().parse::<i64>().ok())
            .filter(|v| *v >= 0)
            .unwrap_or(7);

        let ops_log_tail_limit_default = env::var("CATNAP_OPS_LOG_TAIL_LIMIT_DEFAULT")
            .ok()
            .and_then(|v| v.trim().parse::<i64>().ok())
            .filter(|v| (1..=500).contains(v))
            .unwrap_or(200);

        let ops_queue_task_limit_default = env::var("CATNAP_OPS_QUEUE_TASK_LIMIT_DEFAULT")
            .ok()
            .and_then(|v| v.trim().parse::<i64>().ok())
            .filter(|v| (1..=500).contains(v))
            .unwrap_or(200);

        Self {
            bind_addr: env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:18080".to_string()),
            effective_version,
            repo_url,
            github_api_base_url,
            update_check_repo,
            update_check_enabled,
            update_check_ttl_seconds,
            upstream_cart_url: env::var("CATNAP_UPSTREAM_CART_URL")
                .unwrap_or_else(|_| "https://lazycats.vip/cart".to_string()),
            telegram_api_base_url,
            auth_user_header,
            dev_user_id,
            default_poll_interval_minutes,
            default_poll_jitter_pct,
            log_retention_days,
            log_retention_max_rows,
            ops_worker_concurrency,
            ops_sse_replay_window_seconds,
            ops_log_retention_days,
            ops_log_tail_limit_default,
            ops_queue_task_limit_default,
            db_url: env::var("CATNAP_DB_URL").unwrap_or_else(|_| "sqlite:catnap.db".to_string()),
            web_push_vapid_public_key: env::var("CATNAP_WEB_PUSH_VAPID_PUBLIC_KEY")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty()),
            web_push_vapid_private_key: env::var("CATNAP_WEB_PUSH_VAPID_PRIVATE_KEY")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty()),
            web_push_vapid_subject: env::var("CATNAP_WEB_PUSH_VAPID_SUBJECT")
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty()),
            allow_insecure_local_web_push_endpoints: false,
        }
    }
}

fn parse_bool(s: &str) -> Option<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}
