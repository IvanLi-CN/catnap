use crate::models::{LatestReleaseView, UpdateCheckResponse};
use crate::RuntimeConfig;
use reqwest::header;
use serde::Deserialize;
use std::sync::Arc;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct UpdateChecker {
    cfg: RuntimeConfig,
    client: reqwest::Client,
    cache: Arc<Mutex<Option<Cached>>>,
}

#[derive(Clone)]
struct Cached {
    checked_at: OffsetDateTime,
    value: UpdateCheckResponse,
}

impl UpdateChecker {
    pub fn new(cfg: RuntimeConfig) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("catnap")
            .default_headers({
                let mut h = header::HeaderMap::new();
                h.insert(
                    header::ACCEPT,
                    header::HeaderValue::from_static("application/vnd.github+json"),
                );
                h
            })
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            cfg,
            client,
            cache: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn check(&self) -> UpdateCheckResponse {
        let now = OffsetDateTime::now_utc();

        if !self.cfg.update_check_enabled {
            return UpdateCheckResponse {
                current_version: self.cfg.effective_version.clone(),
                latest: None,
                update_available: false,
                checked_at: format_rfc3339(now),
                error: Some("disabled".to_string()),
            };
        }

        let ttl = self.cfg.update_check_ttl_seconds;
        if ttl > 0 {
            if let Some(hit) = self.cache_hit(now, ttl).await {
                return hit;
            }
        }

        let fresh = self.fetch_latest(now).await;
        let mut guard = self.cache.lock().await;
        *guard = Some(Cached {
            checked_at: now,
            value: fresh.clone(),
        });
        fresh
    }

    async fn cache_hit(
        &self,
        now: OffsetDateTime,
        ttl_seconds: i64,
    ) -> Option<UpdateCheckResponse> {
        let guard = self.cache.lock().await;
        let cached = guard.as_ref()?;
        let age = now - cached.checked_at;
        if age.whole_seconds() < ttl_seconds {
            Some(cached.value.clone())
        } else {
            None
        }
    }

    async fn fetch_latest(&self, now: OffsetDateTime) -> UpdateCheckResponse {
        let current_version = self.cfg.effective_version.clone();
        let checked_at = format_rfc3339(now);

        let repo = self.cfg.update_check_repo.trim();
        let api_base = self.cfg.github_api_base_url.trim_end_matches('/');
        let url = format!("{api_base}/repos/{repo}/releases/latest");

        let res = self.client.get(url).send().await;
        let res = match res {
            Ok(r) => r,
            Err(err) => {
                return UpdateCheckResponse {
                    current_version,
                    latest: None,
                    update_available: false,
                    checked_at,
                    error: Some(format!("request failed: {err}")),
                };
            }
        };

        let status = res.status();
        let body = match res.text().await {
            Ok(t) => t,
            Err(err) => {
                return UpdateCheckResponse {
                    current_version,
                    latest: None,
                    update_available: false,
                    checked_at,
                    error: Some(format!("read body failed: {err}")),
                };
            }
        };

        if !status.is_success() {
            return UpdateCheckResponse {
                current_version,
                latest: None,
                update_available: false,
                checked_at,
                error: Some(format!("github status {status}: {body}")),
            };
        }

        let parsed: GitHubLatestRelease = match serde_json::from_str(&body) {
            Ok(v) => v,
            Err(err) => {
                return UpdateCheckResponse {
                    current_version,
                    latest: None,
                    update_available: false,
                    checked_at,
                    error: Some(format!("invalid json: {err}")),
                };
            }
        };

        let tag = parsed.tag_name.trim().to_string();
        let latest_version = normalize_tag_to_version(&tag);
        let latest = LatestReleaseView {
            tag,
            version: latest_version.clone(),
            html_url: parsed.html_url.trim().to_string(),
            published_at: parsed.published_at.map(|v| v.trim().to_string()),
        };

        let current_semver = parse_semver3(&normalize_tag_to_version(&current_version));
        let latest_semver = parse_semver3(&latest_version);

        let update_available = match (current_semver, latest_semver) {
            (Some(c), Some(l)) => l > c,
            _ => false,
        };

        let error = if current_semver.is_none() || latest_semver.is_none() {
            Some("cannot compare versions (non-semver)".to_string())
        } else {
            None
        };

        UpdateCheckResponse {
            current_version,
            latest: Some(latest),
            update_available,
            checked_at,
            error,
        }
    }
}

#[derive(Debug, Deserialize)]
struct GitHubLatestRelease {
    tag_name: String,
    html_url: String,
    published_at: Option<String>,
}

fn normalize_tag_to_version(s: &str) -> String {
    let s = s.trim();
    let s = s.strip_prefix('v').unwrap_or(s);
    let s = s.split_once('-').map(|(a, _)| a).unwrap_or(s);
    let s = s.split_once('+').map(|(a, _)| a).unwrap_or(s);
    s.trim().to_string()
}

fn parse_semver3(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.trim();
    let mut it = s.split('.');
    let major = it.next()?.parse::<u64>().ok()?;
    let minor = it.next()?.parse::<u64>().ok()?;
    let patch = it.next()?.parse::<u64>().ok()?;
    if it.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

fn format_rfc3339(t: OffsetDateTime) -> String {
    t.format(&Rfc3339).unwrap_or_else(|_| t.to_string())
}
