use std::env;

#[derive(Clone, Debug)]
pub struct RuntimeConfig {
    pub bind_addr: String,
    pub effective_version: String,

    pub upstream_cart_url: String,

    /// Base URL for Telegram Bot API. Used to allow local stubs in tests.
    pub telegram_api_base_url: String,

    /// Name of the request header (provided by a trusted reverse proxy) used to identify the user.
    pub auth_user_header: Option<String>,

    pub default_poll_interval_minutes: i64,
    pub default_poll_jitter_pct: f64,

    pub log_retention_days: i64,
    pub log_retention_max_rows: i64,

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

        let telegram_api_base_url = env::var("CATNAP_TELEGRAM_API_BASE_URL")
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "https://api.telegram.org".to_string());

        let auth_user_header = env::var("CATNAP_AUTH_USER_HEADER")
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

        Self {
            bind_addr: env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:18080".to_string()),
            effective_version,
            upstream_cart_url: env::var("CATNAP_UPSTREAM_CART_URL")
                .unwrap_or_else(|_| "https://lazycats.vip/cart".to_string()),
            telegram_api_base_url,
            auth_user_header,
            default_poll_interval_minutes,
            default_poll_jitter_pct,
            log_retention_days,
            log_retention_max_rows,
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
