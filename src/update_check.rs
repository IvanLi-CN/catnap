use crate::config::RuntimeConfig;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::sync::RwLock;

#[derive(Debug, Default, Clone)]
pub struct UpdateCheckCache {
    pub checked_at: Option<String>,
    pub checked_at_unix: Option<i64>,
    pub etag: Option<String>,
    pub latest_version: Option<String>,
    pub latest_url: Option<String>,
    pub last_error: Option<String>,
}

pub fn new_cache() -> Arc<RwLock<UpdateCheckCache>> {
    Arc::new(RwLock::new(UpdateCheckCache::default()))
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
}

fn now_rfc3339_and_unix() -> (String, i64) {
    let now = OffsetDateTime::now_utc();
    let unix = now.unix_timestamp();
    let s = now
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string());
    (s, unix)
}

fn normalize_semver_tag(tag: &str) -> Option<String> {
    let t = tag.trim();
    let t = t.strip_prefix('v').unwrap_or(t);
    let mut it = t.split('.');
    let major = it.next()?.parse::<u64>().ok()?;
    let minor = it.next()?.parse::<u64>().ok()?;
    let patch = it.next()?.parse::<u64>().ok()?;
    if it.next().is_some() {
        return None;
    }
    Some(format!("{major}.{minor}.{patch}"))
}

fn parse_semver_triplet(v: &str) -> Option<(u64, u64, u64)> {
    let t = v.trim();
    let t = t.strip_prefix('v').unwrap_or(t);
    let mut it = t.split('.');
    let major = it.next()?.parse::<u64>().ok()?;
    let minor = it.next()?.parse::<u64>().ok()?;
    let patch = it.next()?.parse::<u64>().ok()?;
    if it.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

pub fn is_update_available(current: &str, latest: &str) -> bool {
    let Some(a) = parse_semver_triplet(current) else {
        return false;
    };
    let Some(b) = parse_semver_triplet(latest) else {
        return false;
    };
    b > a
}

fn build_latest_release_url(cfg: &RuntimeConfig) -> Result<String, String> {
    let repo = cfg.update_repo.trim();
    if repo.is_empty() {
        return Err("missing CATNAP_UPDATE_REPO".to_string());
    }
    if !repo.contains('/') {
        return Err("invalid CATNAP_UPDATE_REPO (expected owner/repo)".to_string());
    }

    let base = cfg.github_api_base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err("invalid CATNAP_GITHUB_API_BASE_URL".to_string());
    }

    Ok(format!("{base}/repos/{repo}/releases/latest"))
}

pub async fn maybe_refresh(
    cfg: &RuntimeConfig,
    cache: &Arc<RwLock<UpdateCheckCache>>,
    force: bool,
) {
    if !cfg.update_check_enabled {
        return;
    }

    let now_unix = OffsetDateTime::now_utc().unix_timestamp();
    let should_refresh = if force {
        true
    } else {
        let ttl = cfg.update_check_ttl_seconds.max(0);
        let checked_at = cache.read().await.checked_at_unix;
        match checked_at {
            None => true,
            Some(checked_at) => now_unix.saturating_sub(checked_at) >= ttl,
        }
    };

    if !should_refresh {
        return;
    }

    let prev = cache.read().await.clone();
    let (checked_at, checked_at_unix) = now_rfc3339_and_unix();

    let mut next = prev.clone();
    next.checked_at = Some(checked_at);
    next.checked_at_unix = Some(checked_at_unix);

    let url = match build_latest_release_url(cfg) {
        Ok(url) => url,
        Err(err) => {
            next.last_error = Some(err);
            *cache.write().await = next;
            return;
        }
    };

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_millis(
            cfg.update_check_timeout_ms.max(1) as u64
        ))
        .user_agent(format!("catnap/{}", cfg.effective_version))
        .build()
    {
        Ok(c) => c,
        Err(_) => {
            next.last_error = Some("update-check client init failed".to_string());
            *cache.write().await = next;
            return;
        }
    };

    let mut req = client
        .get(url)
        .header("accept", "application/vnd.github+json");
    if let Some(etag) = prev.etag.as_deref() {
        req = req.header(reqwest::header::IF_NONE_MATCH, etag);
    }

    let resp = match req.send().await {
        Ok(resp) => resp,
        Err(_) => {
            next.last_error = Some("update-check request failed".to_string());
            *cache.write().await = next;
            return;
        }
    };

    if resp.status() == reqwest::StatusCode::NOT_MODIFIED {
        next.last_error = None;
        *cache.write().await = next;
        return;
    }

    if !resp.status().is_success() {
        next.last_error = Some(format!("update-check http {}", resp.status().as_u16()));
        *cache.write().await = next;
        return;
    }

    let etag = resp
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    if etag.is_some() {
        next.etag = etag;
    }

    let json = match resp.json::<GitHubRelease>().await {
        Ok(json) => json,
        Err(_) => {
            next.last_error = Some("update-check response parse failed".to_string());
            *cache.write().await = next;
            return;
        }
    };

    next.latest_url = Some(json.html_url);

    match normalize_semver_tag(&json.tag_name) {
        Some(v) => {
            next.latest_version = Some(v);
            next.last_error = None;
        }
        None => {
            next.latest_version = None;
            next.last_error = Some(format!("unsupported latest tag: {}", json.tag_name));
        }
    }

    *cache.write().await = next;
}
