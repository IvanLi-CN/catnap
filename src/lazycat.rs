use crate::app::AppState;
use crate::db::{
    self, LazycatAccountRow, LazycatMachineDetailRecord, LazycatPortMappingRecord,
    LazycatSiteMachineRecord,
};
use crate::models::{LazycatAccountView, LazycatMachineView, LazycatMachinesResponse};
use anyhow::{anyhow, Context};
use futures_util::stream::{self, StreamExt};
use reqwest::header::{COOKIE, LOCATION, SET_COOKIE};
use reqwest::{Method, Url};
use scraper::{Html, Selector};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tracing::warn;

const SITE_REQUEST_TIMEOUT_MS: u64 = 10_000;

#[derive(Debug, Clone, Default)]
struct CookieJar {
    pairs: Vec<(String, String)>,
}

impl CookieJar {
    fn from_pairs(pairs: Vec<(String, String)>) -> Self {
        Self { pairs }
    }

    fn to_pairs(&self) -> Vec<(String, String)> {
        self.pairs.clone()
    }

    fn clear(&mut self) {
        self.pairs.clear();
    }

    fn header_value(&self) -> Option<String> {
        if self.pairs.is_empty() {
            return None;
        }
        Some(
            self.pairs
                .iter()
                .map(|(name, value)| format!("{name}={value}"))
                .collect::<Vec<_>>()
                .join("; "),
        )
    }

    fn set(&mut self, name: &str, value: &str) {
        if let Some((_, existing)) = self.pairs.iter_mut().find(|(key, _)| key == name) {
            *existing = value.to_string();
            return;
        }
        self.pairs.push((name.to_string(), value.to_string()));
    }

    fn remove(&mut self, name: &str) {
        self.pairs.retain(|(key, _)| key != name);
    }

    fn update_from_headers(&mut self, headers: &reqwest::header::HeaderMap) {
        for raw in headers.get_all(SET_COOKIE) {
            let Ok(raw) = raw.to_str() else {
                continue;
            };
            let Some((cookie_pair, _)) = raw.split_once(';') else {
                continue;
            };
            let Some((name, value)) = cookie_pair.split_once('=') else {
                continue;
            };
            let name = name.trim();
            if name.is_empty() {
                continue;
            }
            let lowered = raw.to_ascii_lowercase();
            if lowered.contains("max-age=0") || lowered.contains("expires=thu, 01-jan-1970") {
                self.remove(name);
                continue;
            }
            self.set(name, value.trim());
        }
    }
}

#[derive(Debug)]
struct SyncActiveGuard {
    user_id: String,
    active: Arc<tokio::sync::Mutex<HashSet<String>>>,
}

impl Drop for SyncActiveGuard {
    fn drop(&mut self) {
        let user_id = self.user_id.clone();
        let active = self.active.clone();
        tokio::spawn(async move {
            active.lock().await.remove(&user_id);
        });
    }
}

#[derive(Debug, Clone)]
struct LazycatSession {
    cookies: CookieJar,
    last_login_at: Option<String>,
}

impl LazycatSession {
    fn from_account(account: &LazycatAccountRow) -> Self {
        Self {
            cookies: CookieJar::from_pairs(account.cookies()),
            last_login_at: None,
        }
    }

    fn cookies_json(&self) -> anyhow::Result<Option<String>> {
        let pairs = self.cookies.to_pairs();
        if pairs.is_empty() {
            return Ok(None);
        }
        Ok(Some(serde_json::to_string(&pairs)?))
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct PanelSyncSummary {
    attempted: usize,
    successful: usize,
    failed: usize,
}

impl PanelSyncSummary {
    fn record(&mut self, attempted: bool, successful: bool) {
        if !attempted {
            return;
        }
        self.attempted += 1;
        if successful {
            self.successful += 1;
        } else {
            self.failed += 1;
        }
    }
}

#[derive(Debug)]
struct SiteResponse {
    url: Url,
    body: String,
}

#[derive(Debug)]
struct ClientareaPage {
    service_ids: Vec<i64>,
    total_pages: usize,
}

#[derive(Debug, Clone)]
struct ParsedHostDetail {
    site: LazycatSiteMachineRecord,
    has_info_module: bool,
    has_nat_acl_module: bool,
}

#[derive(Debug, Clone)]
struct ContainerPanelSnapshot {
    panel_url: String,
    panel_hash: String,
}

#[derive(Debug, Clone)]
struct PanelDetailSnapshot {
    traffic_used_gb: Option<f64>,
    traffic_limit_gb: Option<f64>,
    traffic_reset_day: Option<i64>,
    traffic_last_reset_at: Option<String>,
    traffic_display: Option<String>,
}

#[derive(Debug, Clone)]
struct PanelSyncResult {
    detail: LazycatMachineDetailRecord,
    port_mappings: Vec<LazycatPortMappingRecord>,
}

#[derive(Debug, Clone)]
struct LazycatService {
    base_url: Url,
    site_client: reqwest::Client,
    panel_timeout_ms: i64,
    allow_invalid_tls: bool,
}

impl LazycatService {
    fn new(config: &crate::config::RuntimeConfig) -> anyhow::Result<Self> {
        let base_url = Url::parse(&config.lazycat_base_url).with_context(|| {
            format!(
                "invalid CATNAP_LAZYCAT_BASE_URL: {}",
                config.lazycat_base_url
            )
        })?;
        let site_client = reqwest::Client::builder()
            .user_agent("catnap/0.1 (+https://example.invalid)")
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_millis(SITE_REQUEST_TIMEOUT_MS))
            .build()?;
        Ok(Self {
            base_url,
            site_client,
            panel_timeout_ms: config.lazycat_panel_timeout_ms,
            allow_invalid_tls: config.lazycat_allow_invalid_tls,
        })
    }

    async fn validate_login(
        &self,
        email: &str,
        password: &str,
    ) -> anyhow::Result<(CookieJar, String)> {
        let mut session = LazycatSession {
            cookies: CookieJar::default(),
            last_login_at: None,
        };
        self.login(&mut session, email, password).await?;
        Ok((
            session.cookies,
            session
                .last_login_at
                .unwrap_or_else(current_timestamp_rfc3339),
        ))
    }

    async fn sync_site(
        &self,
        account: &mut LazycatAccountRow,
    ) -> anyhow::Result<Vec<LazycatSiteMachineRecord>> {
        let mut session = LazycatSession::from_account(account);
        let service_ids = self
            .list_service_ids(&mut session, &account.email, &account.password)
            .await?;
        let site_sync_at = current_timestamp_rfc3339();
        let mut machines = Vec::new();

        for service_id in service_ids {
            let detail_json = self
                .site_get_json_with_reauth(
                    &mut session,
                    &account.email,
                    &account.password,
                    &format!("/host/dedicatedserver?host_id={service_id}"),
                    true,
                )
                .await
                .with_context(|| format!("fetch host detail failed for service {service_id}"))?;
            let mut parsed = parse_host_detail(service_id, &detail_json, &site_sync_at)?;

            let renew_price = self
                .site_get_html_with_reauth(
                    &mut session,
                    &account.email,
                    &account.password,
                    &format!("/servicedetail?id={service_id}&action=renew"),
                    false,
                )
                .await
                .ok()
                .and_then(|html| parse_renew_price(&html));
            if renew_price.is_some() {
                parsed.site.renew_price = renew_price;
            }

            if parsed.has_info_module {
                let info_html = self
                    .site_get_html_with_reauth(
                        &mut session,
                        &account.email,
                        &account.password,
                        &format!("/provision/custom/content?id={service_id}&key=info"),
                        false,
                    )
                    .await
                    .ok();
                if let Some(info_html) = info_html.as_deref() {
                    if let Some(panel) = parse_container_panel_snapshot(info_html) {
                        parsed.site.panel_url = Some(panel.panel_url);
                        parsed.site.panel_hash = Some(panel.panel_hash);
                        if !parsed.has_nat_acl_module {
                            parsed.site.panel_kind = Some("container".to_string());
                        }
                    }
                }
            }

            machines.push(parsed.site);
        }

        account.cookies_json = session.cookies_json()?;
        if let Some(last_login_at) = session.last_login_at {
            account.last_authenticated_at = Some(last_login_at);
        }
        account.last_site_sync_at = Some(site_sync_at);
        Ok(machines)
    }

    async fn sync_machine_detail(
        &self,
        machine: &db::LazycatMachineRow,
        account: &LazycatAccountRow,
    ) -> anyhow::Result<Option<PanelSyncResult>> {
        match machine.panel_kind.as_deref() {
            Some("container") => {
                let panel_url = machine
                    .panel_url
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| anyhow!("missing panel url"))?;
                let panel_hash = machine
                    .panel_hash
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| anyhow!("missing panel hash"))?;
                self.sync_container_panel(machine.service_id, panel_url, panel_hash)
                    .await
                    .map(Some)
            }
            Some("nat") => self
                .sync_nat_panel(machine.service_id, account)
                .await
                .map(Some),
            _ => Ok(None),
        }
    }

    async fn sync_container_panel(
        &self,
        service_id: i64,
        panel_url: &str,
        panel_hash: &str,
    ) -> anyhow::Result<PanelSyncResult> {
        let detail_sync_at = current_timestamp_rfc3339();
        let panel_dashboard_url = normalize_panel_dashboard_url(panel_url)?;
        let dashboard_url = Url::parse(&panel_dashboard_url)?;

        let info_json = self
            .panel_get_json(&dashboard_url, panel_hash, "/api/container/info")
            .await?;
        let panel_detail = parse_panel_detail(&info_json)?;

        let ipv4_json = self
            .panel_get_json(
                &dashboard_url,
                panel_hash,
                "/api/container/port-mapping?version=v4",
            )
            .await?;
        let ipv6_json = self
            .panel_get_json(
                &dashboard_url,
                panel_hash,
                "/api/container/port-mapping?version=v6",
            )
            .await?;

        let mut port_mappings = parse_container_port_mappings(&ipv4_json, "v4");
        port_mappings.extend(parse_container_port_mappings(&ipv6_json, "v6"));

        Ok(PanelSyncResult {
            detail: LazycatMachineDetailRecord {
                service_id,
                panel_kind: Some("container".to_string()),
                panel_url: Some(panel_dashboard_url),
                panel_hash: Some(panel_hash.to_string()),
                traffic_used_gb: panel_detail.traffic_used_gb,
                traffic_limit_gb: panel_detail.traffic_limit_gb,
                traffic_reset_day: panel_detail.traffic_reset_day,
                traffic_last_reset_at: panel_detail.traffic_last_reset_at,
                traffic_display: panel_detail.traffic_display,
                detail_state: "ready".to_string(),
                detail_error: None,
                last_panel_sync_at: detail_sync_at,
            },
            port_mappings,
        })
    }

    async fn sync_nat_panel(
        &self,
        service_id: i64,
        account: &LazycatAccountRow,
    ) -> anyhow::Result<PanelSyncResult> {
        let detail_sync_at = current_timestamp_rfc3339();
        let mut session = LazycatSession::from_account(account);
        let nat_json = self
            .site_get_json_with_reauth(
                &mut session,
                &account.email,
                &account.password,
                &format!("/provision/custom/content?id={service_id}&key=nat_acl&action=natlist"),
                false,
            )
            .await?;
        ensure_nat_response_ok(&nat_json)?;
        let port_mappings = parse_nat_port_mappings(&nat_json);
        Ok(PanelSyncResult {
            detail: LazycatMachineDetailRecord {
                service_id,
                panel_kind: Some("nat".to_string()),
                panel_url: None,
                panel_hash: None,
                traffic_used_gb: None,
                traffic_limit_gb: None,
                traffic_reset_day: None,
                traffic_last_reset_at: None,
                traffic_display: None,
                detail_state: "ready".to_string(),
                detail_error: None,
                last_panel_sync_at: detail_sync_at,
            },
            port_mappings,
        })
    }

    async fn list_service_ids(
        &self,
        session: &mut LazycatSession,
        email: &str,
        password: &str,
    ) -> anyhow::Result<Vec<i64>> {
        let mut page = 1_usize;
        let mut total_pages = 1_usize;
        let mut service_ids = Vec::new();
        let mut seen = HashSet::new();

        loop {
            let html = self
                .site_get_html_with_reauth(
                    session,
                    email,
                    password,
                    &format!("/clientarea?action=list&page={page}"),
                    false,
                )
                .await?;
            let parsed = parse_clientarea_page(&html)?;
            total_pages = total_pages.max(parsed.total_pages.max(page));
            for id in parsed.service_ids {
                if seen.insert(id) {
                    service_ids.push(id);
                }
            }
            if page >= total_pages {
                break;
            }
            page += 1;
        }

        Ok(service_ids)
    }

    async fn site_get_html_with_reauth(
        &self,
        session: &mut LazycatSession,
        email: &str,
        password: &str,
        path: &str,
        xhr: bool,
    ) -> anyhow::Result<String> {
        let mut retried = false;
        loop {
            let response = self
                .site_request(session, Method::GET, path, None, xhr)
                .await?;
            if is_login_page(&response.url, &response.body) {
                if retried {
                    return Err(anyhow!(
                        "{}",
                        extract_login_error(&response.body)
                            .unwrap_or_else(|| "懒猫云登录态失效，自动重登失败".to_string())
                    ));
                }
                self.login(session, email, password).await?;
                retried = true;
                continue;
            }
            return Ok(response.body);
        }
    }

    async fn site_get_json_with_reauth(
        &self,
        session: &mut LazycatSession,
        email: &str,
        password: &str,
        path: &str,
        xhr: bool,
    ) -> anyhow::Result<Value> {
        let html = self
            .site_get_html_with_reauth(session, email, password, path, xhr)
            .await?;
        serde_json::from_str::<Value>(&html)
            .with_context(|| format!("invalid lazycat json response from {path}"))
    }

    async fn login(
        &self,
        session: &mut LazycatSession,
        email: &str,
        password: &str,
    ) -> anyhow::Result<()> {
        session.cookies.clear();
        let login_page = self
            .site_request(session, Method::GET, "/login", None, false)
            .await?;
        let token = parse_login_token(&login_page.body)?;
        let form = vec![
            ("token".to_string(), token),
            ("email".to_string(), email.to_string()),
            ("password".to_string(), password.to_string()),
        ];
        let response = self
            .site_request(
                session,
                Method::POST,
                "/login?action=email",
                Some(form),
                false,
            )
            .await?;
        if is_login_page(&response.url, &response.body) {
            return Err(anyhow!(
                "{}",
                extract_login_error(&response.body).unwrap_or_else(|| "懒猫云登录失败".to_string())
            ));
        }
        session.last_login_at = Some(current_timestamp_rfc3339());
        Ok(())
    }

    async fn site_request(
        &self,
        session: &mut LazycatSession,
        mut method: Method,
        path: &str,
        mut form: Option<Vec<(String, String)>>,
        xhr: bool,
    ) -> anyhow::Result<SiteResponse> {
        let mut url = self.base_url.join(path)?;
        for _ in 0..5 {
            let mut request = self.site_client.request(method.clone(), url.clone());
            if let Some(cookie_header) = session.cookies.header_value() {
                request = request.header(COOKIE, cookie_header);
            }
            if xhr {
                request = request.header("x-requested-with", "XMLHttpRequest");
            }
            if let Some(form_data) = form.clone() {
                request = request.form(&form_data);
            }
            let response = request.send().await?;
            session.cookies.update_from_headers(response.headers());

            if response.status().is_redirection() {
                let location = response
                    .headers()
                    .get(LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .ok_or_else(|| anyhow!("lazycat redirect missing location"))?;
                url = url.join(location)?;
                if matches!(
                    response.status(),
                    reqwest::StatusCode::FOUND
                        | reqwest::StatusCode::MOVED_PERMANENTLY
                        | reqwest::StatusCode::SEE_OTHER
                ) {
                    method = Method::GET;
                    form = None;
                }
                continue;
            }

            let body = response.text().await?;
            return Ok(SiteResponse { url, body });
        }

        Err(anyhow!("lazycat redirect loop"))
    }

    async fn panel_get_json(
        &self,
        dashboard_url: &Url,
        panel_hash: &str,
        endpoint: &str,
    ) -> anyhow::Result<Value> {
        let mut endpoint_url = dashboard_url.clone();
        endpoint_url.set_query(None);
        endpoint_url.set_path(endpoint);

        match self
            .panel_request_json(&endpoint_url, panel_hash, false)
            .await
        {
            Ok(value) => Ok(value),
            Err(err) if should_retry_insecure(&endpoint_url, &err, self.allow_invalid_tls) => {
                self.panel_request_json(&endpoint_url, panel_hash, true)
                    .await
            }
            Err(err) => Err(err),
        }
    }

    async fn panel_request_json(
        &self,
        endpoint_url: &Url,
        panel_hash: &str,
        insecure: bool,
    ) -> anyhow::Result<Value> {
        let client = reqwest::Client::builder()
            .user_agent("catnap/0.1 (+https://example.invalid)")
            .timeout(std::time::Duration::from_millis(
                self.panel_timeout_ms.max(500) as u64,
            ))
            .danger_accept_invalid_certs(insecure)
            .danger_accept_invalid_hostnames(insecure)
            .build()?;

        let response = client
            .get(endpoint_url.clone())
            .header("x-container-hash", panel_hash)
            .header("x-requested-with", "XMLHttpRequest")
            .send()
            .await?;
        let body = response.text().await?;
        serde_json::from_str::<Value>(&body)
            .with_context(|| format!("invalid panel json response from {endpoint_url}"))
    }
}

pub fn empty_account_view() -> LazycatAccountView {
    LazycatAccountView {
        connected: false,
        email: None,
        state: "disconnected".to_string(),
        machine_count: 0,
        last_site_sync_at: None,
        last_panel_sync_at: None,
        last_error: None,
    }
}

pub async fn get_account_view(
    state: &AppState,
    user_id: &str,
) -> anyhow::Result<LazycatAccountView> {
    match db::get_lazycat_account(&state.db, user_id).await? {
        Some(account) => {
            let machine_count = db::count_lazycat_machines(&state.db, user_id).await?;
            Ok(account.to_view(machine_count))
        }
        None => Ok(empty_account_view()),
    }
}

pub async fn get_machines_response(
    state: &AppState,
    user_id: &str,
) -> anyhow::Result<LazycatMachinesResponse> {
    let account = get_account_view(state, user_id).await?;
    let machines = db::list_lazycat_machines(&state.db, user_id).await?;
    let port_mappings = db::list_lazycat_port_mappings(&state.db, user_id).await?;
    let mut mappings_by_service = HashMap::<i64, Vec<crate::models::LazycatPortMappingView>>::new();
    for (service_id, mapping) in port_mappings {
        mappings_by_service
            .entry(service_id)
            .or_default()
            .push(mapping);
    }
    let items = machines
        .iter()
        .map(|machine| {
            let service_mappings = mappings_by_service
                .remove(&machine.service_id)
                .unwrap_or_default();
            machine.to_view(service_mappings)
        })
        .collect::<Vec<LazycatMachineView>>();
    Ok(LazycatMachinesResponse { account, items })
}

pub async fn login_account(
    state: &AppState,
    user_id: &str,
    email: &str,
    password: &str,
) -> anyhow::Result<LazycatAccountView> {
    let email = email.trim();
    if email.is_empty() || password.is_empty() {
        return Err(anyhow!("邮箱和密码不能为空"));
    }
    let Some(guard) = try_acquire_sync_guard(state, user_id).await else {
        return Err(anyhow!("当前懒猫云同步仍在进行，请稍后重试"));
    };

    let service = LazycatService::new(&state.config)?;
    let (cookies, authenticated_at) = service.validate_login(email, password).await?;
    db::delete_lazycat_account_data(&state.db, user_id).await?;

    let now = current_timestamp_rfc3339();
    let account = LazycatAccountRow {
        user_id: user_id.to_string(),
        email: email.to_string(),
        password: password.to_string(),
        cookies_json: {
            let pairs = cookies.to_pairs();
            if pairs.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&pairs)?)
            }
        },
        state: "syncing".to_string(),
        last_error: None,
        last_authenticated_at: Some(authenticated_at),
        last_site_sync_at: None,
        last_panel_sync_at: None,
        created_at: now.clone(),
        updated_at: now,
    };
    let saved = db::put_lazycat_account(&state.db, &account).await?;
    let machine_count = db::count_lazycat_machines(&state.db, user_id).await?;
    spawn_sync_task(state.clone(), user_id.to_string(), guard);
    Ok(saved.to_view(machine_count))
}

pub async fn request_sync(state: &AppState, user_id: &str) -> anyhow::Result<LazycatAccountView> {
    let Some(mut account) = db::get_lazycat_account(&state.db, user_id).await? else {
        return Err(anyhow!("请先连接懒猫云账号"));
    };
    account.state = "syncing".to_string();
    account.last_error = None;
    account.updated_at = current_timestamp_rfc3339();
    let saved = db::put_lazycat_account(&state.db, &account).await?;
    let machine_count = db::count_lazycat_machines(&state.db, user_id).await?;
    let _ = spawn_sync(state.clone(), user_id.to_string()).await;
    Ok(saved.to_view(machine_count))
}

pub async fn disconnect_account(state: &AppState, user_id: &str) -> anyhow::Result<()> {
    db::delete_lazycat_account_data(&state.db, user_id).await
}

pub async fn maybe_spawn_due_sync(state: &AppState, user_id: &str) -> anyhow::Result<()> {
    let Some(account) = db::get_lazycat_account(&state.db, user_id).await? else {
        return Ok(());
    };
    let now = OffsetDateTime::now_utc();
    let site_due = sync_due(
        account.last_site_sync_at.as_deref(),
        now,
        state.config.lazycat_site_sync_interval_minutes,
    );
    let panel_due = sync_due(
        account.last_panel_sync_at.as_deref(),
        now,
        state.config.lazycat_panel_sync_interval_minutes,
    );
    if site_due || panel_due {
        let _ = spawn_sync(state.clone(), user_id.to_string()).await;
    }
    Ok(())
}

pub async fn spawn_sync(state: AppState, user_id: String) -> anyhow::Result<bool> {
    let Some(guard) = try_acquire_sync_guard(&state, &user_id).await else {
        return Ok(false);
    };
    spawn_sync_task(state, user_id, guard);
    Ok(true)
}

fn spawn_sync_task(state: AppState, user_id: String, guard: SyncActiveGuard) {
    tokio::spawn(async move {
        let _guard = guard;
        if let Err(err) = sync_user_inner(&state, &user_id).await {
            warn!(user_id, error = %err, "lazycat sync failed");
        }
    });
}

async fn try_acquire_sync_guard(state: &AppState, user_id: &str) -> Option<SyncActiveGuard> {
    let mut active = state.lazycat_sync_users.lock().await;
    if active.contains(user_id) {
        return None;
    }
    active.insert(user_id.to_string());
    Some(SyncActiveGuard {
        user_id: user_id.to_string(),
        active: state.lazycat_sync_users.clone(),
    })
}

async fn sync_user_inner(state: &AppState, user_id: &str) -> anyhow::Result<()> {
    let Some(mut account) = db::get_lazycat_account(&state.db, user_id).await? else {
        return Ok(());
    };
    let sync_generation = account.created_at.clone();
    account.state = "syncing".to_string();
    account.last_error = None;
    account.updated_at = current_timestamp_rfc3339();
    account = db::put_lazycat_account(&state.db, &account).await?;

    let service = LazycatService::new(&state.config)?;
    let site_machines = match service.sync_site(&mut account).await {
        Ok(machines) => machines,
        Err(err) => {
            if !sync_account_is_current(&state.db, user_id, &sync_generation).await? {
                return Ok(());
            }
            account.state = "error".to_string();
            account.last_error = Some(err.to_string());
            account.updated_at = current_timestamp_rfc3339();
            db::put_lazycat_account(&state.db, &account).await?;
            return Err(err);
        }
    };

    if !sync_account_is_current(&state.db, user_id, &sync_generation).await? {
        return Ok(());
    }
    db::put_lazycat_account(&state.db, &account).await?;
    db::upsert_lazycat_site_machines(&state.db, user_id, &site_machines).await?;

    let existing_machines = db::list_lazycat_machines(&state.db, user_id).await?;
    let existing_by_id = existing_machines
        .into_iter()
        .map(|machine| (machine.service_id, machine))
        .collect::<HashMap<_, _>>();

    let concurrency = state.config.lazycat_panel_concurrency.max(1);
    let db_pool = state.db.clone();
    let panel_summary = stream::iter(site_machines.into_iter())
        .map(|site_machine| {
            let service = service.clone();
            let db_pool = db_pool.clone();
            let user_id = user_id.to_string();
            let account = account.clone();
            let existing_by_id = existing_by_id.clone();
            let sync_generation = sync_generation.clone();
            async move {
                let Some(machine) = existing_by_id.get(&site_machine.service_id) else {
                    return Ok::<(bool, bool), anyhow::Error>((false, false));
                };
                let sync_now = current_timestamp_rfc3339();
                match service.sync_machine_detail(machine, &account).await {
                    Ok(Some(result)) => {
                        if !sync_account_is_current(&db_pool, &user_id, &sync_generation).await? {
                            return Ok((false, false));
                        }
                        db::update_lazycat_machine_detail(&db_pool, &user_id, &result.detail)
                            .await?;
                        let families = if result.detail.panel_kind.as_deref() == Some("container") {
                            vec!["v4".to_string(), "v6".to_string()]
                        } else {
                            vec!["nat".to_string()]
                        };
                        for family in families {
                            let items = result
                                .port_mappings
                                .iter()
                                .filter(|mapping| mapping.family == family)
                                .cloned()
                                .collect::<Vec<_>>();
                            db::replace_lazycat_port_mappings(
                                &db_pool,
                                &user_id,
                                machine.service_id,
                                &family,
                                &items,
                                &result.detail.last_panel_sync_at,
                            )
                            .await?;
                        }
                        for stale_family in
                            stale_port_mapping_families(result.detail.panel_kind.as_deref())
                        {
                            db::replace_lazycat_port_mappings(
                                &db_pool,
                                &user_id,
                                machine.service_id,
                                stale_family,
                                &[],
                                &result.detail.last_panel_sync_at,
                            )
                            .await?;
                        }
                        Ok((true, true))
                    }
                    Ok(None) => {
                        if !sync_account_is_current(&db_pool, &user_id, &sync_generation).await? {
                            return Ok((false, false));
                        }
                        let detail = LazycatMachineDetailRecord {
                            service_id: machine.service_id,
                            panel_kind: machine.panel_kind.clone(),
                            panel_url: machine.panel_url.clone(),
                            panel_hash: machine.panel_hash.clone(),
                            traffic_used_gb: machine.traffic_used_gb,
                            traffic_limit_gb: machine.traffic_limit_gb,
                            traffic_reset_day: machine.traffic_reset_day,
                            traffic_last_reset_at: machine.traffic_last_reset_at.clone(),
                            traffic_display: machine.traffic_display.clone(),
                            detail_state: "ready".to_string(),
                            detail_error: None,
                            last_panel_sync_at: sync_now,
                        };
                        db::update_lazycat_machine_detail(&db_pool, &user_id, &detail).await?;
                        for stale_family in ["v4", "v6", "nat"] {
                            db::replace_lazycat_port_mappings(
                                &db_pool,
                                &user_id,
                                machine.service_id,
                                stale_family,
                                &[],
                                &detail.last_panel_sync_at,
                            )
                            .await?;
                        }
                        Ok((false, false))
                    }
                    Err(err) => {
                        if !sync_account_is_current(&db_pool, &user_id, &sync_generation).await? {
                            return Ok((false, false));
                        }
                        let detail = LazycatMachineDetailRecord {
                            service_id: machine.service_id,
                            panel_kind: machine.panel_kind.clone(),
                            panel_url: machine.panel_url.clone(),
                            panel_hash: machine.panel_hash.clone(),
                            traffic_used_gb: machine.traffic_used_gb,
                            traffic_limit_gb: machine.traffic_limit_gb,
                            traffic_reset_day: machine.traffic_reset_day,
                            traffic_last_reset_at: machine.traffic_last_reset_at.clone(),
                            traffic_display: machine.traffic_display.clone(),
                            detail_state: if machine.last_panel_sync_at.is_some() {
                                "stale".to_string()
                            } else {
                                "error".to_string()
                            },
                            detail_error: Some(err.to_string()),
                            last_panel_sync_at: machine
                                .last_panel_sync_at
                                .clone()
                                .unwrap_or_else(|| sync_now.clone()),
                        };
                        db::update_lazycat_machine_detail(&db_pool, &user_id, &detail).await?;
                        Ok((true, false))
                    }
                }
            }
        })
        .buffer_unordered(concurrency)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .fold(
            PanelSyncSummary::default(),
            |mut summary, (attempted, successful)| {
                summary.record(attempted, successful);
                summary
            },
        );

    if !sync_account_is_current(&state.db, user_id, &sync_generation).await? {
        return Ok(());
    }
    apply_panel_sync_summary(&mut account, panel_summary, &current_timestamp_rfc3339());
    account.updated_at = current_timestamp_rfc3339();
    db::put_lazycat_account(&state.db, &account).await?;
    Ok(())
}

async fn sync_account_is_current(
    db: &sqlx::SqlitePool,
    user_id: &str,
    created_at: &str,
) -> anyhow::Result<bool> {
    Ok(db::get_lazycat_account(db, user_id)
        .await?
        .is_some_and(|account| account.created_at == created_at))
}

fn apply_panel_sync_summary(
    account: &mut LazycatAccountRow,
    summary: PanelSyncSummary,
    finished_at: &str,
) {
    if summary.attempted > 0 && summary.successful == 0 {
        account.state = "error".to_string();
        account.last_error = Some("面板同步全部失败，已保留最近一次成功缓存".to_string());
        return;
    }

    account.state = "ready".to_string();
    account.last_panel_sync_at = Some(finished_at.to_string());
    account.last_error = if summary.failed > 0 {
        Some(format!(
            "部分面板同步失败（{}/{}），已保留最近一次成功缓存",
            summary.failed, summary.attempted
        ))
    } else {
        None
    };
}

fn stale_port_mapping_families(panel_kind: Option<&str>) -> &'static [&'static str] {
    match panel_kind {
        Some("container") => &["nat"],
        Some("nat") => &["v4", "v6"],
        _ => &["v4", "v6", "nat"],
    }
}

fn sync_due(last_sync_at: Option<&str>, now: OffsetDateTime, interval_minutes: i64) -> bool {
    let Some(last_sync_at) = last_sync_at else {
        return true;
    };
    let Ok(last_sync) = OffsetDateTime::parse(last_sync_at, &Rfc3339) else {
        return true;
    };
    now - last_sync >= time::Duration::minutes(interval_minutes.max(1))
}

fn selector(input: &str) -> anyhow::Result<Selector> {
    Selector::parse(input).map_err(|_| anyhow!("invalid selector: {input}"))
}

fn current_timestamp_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn parse_login_token(html: &str) -> anyhow::Result<String> {
    let document = Html::parse_document(html);
    let input_selector = selector(r#"input[name="token"]"#)?;
    let token = document
        .select(&input_selector)
        .filter_map(|node| node.value().attr("value"))
        .map(str::trim)
        .find(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("懒猫云登录页缺少 token"))?;
    Ok(token.to_string())
}

fn is_login_page(url: &Url, body: &str) -> bool {
    if url.path().eq_ignore_ascii_case("/login") {
        return true;
    }
    body.contains("id=\"loginForm\"") && body.contains("/login?action=email")
}

fn extract_login_error(body: &str) -> Option<String> {
    let document = Html::parse_document(body);
    let selectors = [
        ".alert-danger",
        ".alert",
        ".invalid-feedback",
        ".text-danger",
    ];
    for raw_selector in selectors {
        let Ok(sel) = Selector::parse(raw_selector) else {
            continue;
        };
        for node in document.select(&sel) {
            let text = node.text().collect::<String>().trim().replace('\n', " ");
            let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    if body.contains("邮箱未注册") {
        return Some("邮箱未注册".to_string());
    }
    None
}

fn parse_clientarea_page(html: &str) -> anyhow::Result<ClientareaPage> {
    let document = Html::parse_document(html);
    let link_selector = selector("a[href]")?;
    let base_url = Url::parse("https://lxc.lazycat.wiki")?;
    let mut service_ids = Vec::new();
    let mut seen = HashSet::new();
    let mut total_pages = 1_usize;

    for link in document.select(&link_selector) {
        let Some(href) = link.value().attr("href") else {
            continue;
        };
        let Ok(url) = base_url.join(href) else {
            continue;
        };
        if url.path() == "/servicedetail" {
            if let Some(id) = url
                .query_pairs()
                .find_map(|(key, value)| (key == "id").then(|| value.to_string()))
                .and_then(|value| value.parse::<i64>().ok())
            {
                if seen.insert(id) {
                    service_ids.push(id);
                }
            }
        }
        if url.path() == "/clientarea" {
            if let Some(page) = url
                .query_pairs()
                .find_map(|(key, value)| (key == "page").then(|| value.to_string()))
                .and_then(|value| value.parse::<usize>().ok())
            {
                total_pages = total_pages.max(page);
            }
        }
    }

    Ok(ClientareaPage {
        service_ids,
        total_pages,
    })
}

fn parse_host_detail(
    service_id: i64,
    json: &Value,
    last_site_sync_at: &str,
) -> anyhow::Result<ParsedHostDetail> {
    let data = json
        .get("data")
        .ok_or_else(|| anyhow!("missing lazycat host detail data"))?;
    let host_data = data
        .get("host_data")
        .ok_or_else(|| anyhow!("missing lazycat host_data"))?;

    let module_keys = data
        .get("module_client_area")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("key").and_then(Value::as_str))
                .map(|value| value.to_string())
                .collect::<HashSet<_>>()
        })
        .unwrap_or_default();

    let assigned_ips = match host_data.get("assignedips") {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| value.to_string())
            .collect::<Vec<_>>(),
        Some(Value::String(value)) => value
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(|value| value.to_string())
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };

    let renew_price_fallback = host_data
        .get("amount_desc")
        .and_then(Value::as_str)
        .map(|value| {
            format_price_display(
                value,
                host_data.get("billingcycle_desc").and_then(Value::as_str),
            )
        });

    let expires_at = host_data
        .get("nextduedate")
        .and_then(Value::as_i64)
        .and_then(|value| OffsetDateTime::from_unix_timestamp(value).ok())
        .and_then(|value| value.format(&Rfc3339).ok());

    Ok(ParsedHostDetail {
        site: LazycatSiteMachineRecord {
            service_id,
            service_name: host_data
                .get("productname")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            service_code: host_data
                .get("domain")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            status: host_data
                .get("domainstatus")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            os: host_data
                .get("os")
                .and_then(Value::as_str)
                .map(|value| value.to_string()),
            primary_address: host_data
                .get("dedicatedip")
                .and_then(Value::as_str)
                .map(|value| value.to_string()),
            extra_addresses: assigned_ips,
            billing_cycle: host_data
                .get("billingcycle")
                .and_then(Value::as_str)
                .map(|value| value.to_string()),
            renew_price: renew_price_fallback,
            first_price: host_data
                .get("firstpaymentamount_desc")
                .and_then(Value::as_str)
                .map(|value| value.to_string()),
            expires_at,
            panel_kind: if module_keys.contains("nat_acl") {
                Some("nat".to_string())
            } else if module_keys.contains("info") {
                Some("container".to_string())
            } else {
                None
            },
            panel_url: None,
            panel_hash: None,
            last_site_sync_at: last_site_sync_at.to_string(),
        },
        has_info_module: module_keys.contains("info"),
        has_nat_acl_module: module_keys.contains("nat_acl"),
    })
}

fn format_price_display(amount_desc: &str, billing_cycle_desc: Option<&str>) -> String {
    let amount_desc = amount_desc.trim();
    if let Some(billing_cycle_desc) = billing_cycle_desc
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if amount_desc.contains('/') {
            return amount_desc.to_string();
        }
        return format!("{amount_desc}/{billing_cycle_desc}");
    }
    amount_desc.to_string()
}

fn parse_renew_price(html: &str) -> Option<String> {
    let document = Html::parse_document(html);
    let selectors = [
        r#"label.btn-radio-bill.active[data-amount]"#,
        r#"label.btn-radio-bill[data-amount]"#,
    ];
    for raw_selector in selectors {
        let Ok(sel) = Selector::parse(raw_selector) else {
            continue;
        };
        if let Some(label) = document.select(&sel).next() {
            let amount = label.value().attr("data-amount")?.trim();
            let billing_cycle = label.value().attr("data-billingcycle")?.trim();
            if !amount.is_empty() && !billing_cycle.is_empty() {
                return Some(format!("¥{amount}元/{billing_cycle}"));
            }
        }
    }
    None
}

fn parse_container_panel_snapshot(html: &str) -> Option<ContainerPanelSnapshot> {
    let document = Html::parse_document(html);
    let selectors = [r#"a[href*="hash="]"#, r#"iframe[src*="hash="]"#];
    for raw_selector in selectors {
        let Ok(sel) = Selector::parse(raw_selector) else {
            continue;
        };
        if let Some(node) = document.select(&sel).next() {
            let attr = node
                .value()
                .attr("href")
                .or_else(|| node.value().attr("src"))?;
            let normalized = normalize_panel_dashboard_url(attr).ok()?;
            let url = Url::parse(&normalized).ok()?;
            let hash = url
                .query_pairs()
                .find_map(|(key, value)| (key == "hash").then(|| value.to_string()))?;
            return Some(ContainerPanelSnapshot {
                panel_url: normalized,
                panel_hash: hash,
            });
        };
    }
    None
}

fn normalize_panel_dashboard_url(input: &str) -> anyhow::Result<String> {
    let mut url = Url::parse(input)?;
    if url.path().ends_with("/container/dashboard/base") {
        url.set_path("/container/dashboard");
    }
    Ok(url.to_string())
}

fn parse_panel_detail(json: &Value) -> anyhow::Result<PanelDetailSnapshot> {
    let data = json
        .get("data")
        .ok_or_else(|| anyhow!("missing panel detail data"))?;
    let traffic = data.get("traffic").cloned().unwrap_or(Value::Null);
    let used_gb = traffic
        .get("TotalGB")
        .and_then(Value::as_f64)
        .or_else(|| data.get("traffic_usage_raw").and_then(Value::as_f64));
    let limit_gb = traffic
        .get("LimitGB")
        .and_then(Value::as_f64)
        .or_else(|| data.get("traffic_limit").and_then(Value::as_f64));
    let reset_day = traffic.get("ResetDay").and_then(Value::as_i64);
    Ok(PanelDetailSnapshot {
        traffic_used_gb: used_gb,
        traffic_limit_gb: limit_gb,
        traffic_reset_day: reset_day,
        traffic_last_reset_at: traffic
            .get("LastReset")
            .and_then(Value::as_str)
            .and_then(normalize_datetime_string),
        traffic_display: match (used_gb, limit_gb) {
            (Some(used), Some(limit)) => Some(format!(
                "{used:.2} GB / {} GB",
                if limit.fract() == 0.0 {
                    format!("{limit:.0}")
                } else {
                    format!("{limit:.2}")
                }
            )),
            _ => data
                .get("traffic_usage")
                .and_then(Value::as_str)
                .map(|value| value.to_string()),
        },
    })
}

fn parse_container_port_mappings(json: &Value, family: &str) -> Vec<LazycatPortMappingRecord> {
    let Some(items) = json
        .get("data")
        .and_then(|value| value.get(if family == "v6" { "ipv6" } else { "ipv4" }))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    items
        .iter()
        .map(|item| {
            let mapping_key = item
                .get("id")
                .and_then(Value::as_i64)
                .map(|value| value.to_string())
                .unwrap_or_else(|| {
                    format!(
                        "{}:{}:{}:{}",
                        item.get("public_ip")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                        item.get("public_port")
                            .and_then(Value::as_i64)
                            .unwrap_or_default(),
                        item.get("container_ip")
                            .and_then(Value::as_str)
                            .unwrap_or_default(),
                        item.get("container_port")
                            .and_then(Value::as_i64)
                            .unwrap_or_default()
                    )
                });
            LazycatPortMappingRecord {
                family: family.to_string(),
                mapping_key,
                public_ip: item
                    .get("public_ip")
                    .and_then(Value::as_str)
                    .map(|value| value.to_string()),
                public_port: item.get("public_port").and_then(Value::as_i64),
                public_port_end: item.get("public_port_end").and_then(Value::as_i64),
                private_ip: item
                    .get("container_ip")
                    .and_then(Value::as_str)
                    .map(|value| value.to_string()),
                private_port: item.get("container_port").and_then(Value::as_i64),
                private_port_end: item.get("container_port_end").and_then(Value::as_i64),
                protocol: item
                    .get("protocol")
                    .and_then(Value::as_str)
                    .map(|value| value.to_string()),
                status: item
                    .get("status")
                    .and_then(Value::as_str)
                    .map(|value| value.to_string()),
                description: item
                    .get("description")
                    .and_then(Value::as_str)
                    .map(|value| value.to_string()),
                remote_created_at: item
                    .get("created_at")
                    .and_then(Value::as_str)
                    .and_then(normalize_datetime_string),
                remote_updated_at: item
                    .get("updated_at")
                    .and_then(Value::as_str)
                    .and_then(normalize_datetime_string),
            }
        })
        .collect()
}

fn ensure_nat_response_ok(json: &Value) -> anyhow::Result<()> {
    let code = json.get("code").and_then(Value::as_i64).unwrap_or_default();
    if code == 200 {
        return Ok(());
    }
    let msg = json
        .get("msg")
        .and_then(Value::as_str)
        .unwrap_or("NAT 代理调用失败");
    Err(anyhow!(msg.to_string()))
}

fn parse_nat_port_mappings(json: &Value) -> Vec<LazycatPortMappingRecord> {
    let candidates = [
        json.get("data").and_then(Value::as_array),
        json.get("data")
            .and_then(|value| value.get("list"))
            .and_then(Value::as_array),
        json.get("data")
            .and_then(|value| value.get("items"))
            .and_then(Value::as_array),
        json.get("list").and_then(Value::as_array),
    ];
    let Some(items) = candidates.into_iter().flatten().next() else {
        return Vec::new();
    };
    items
        .iter()
        .map(|item| {
            let public_port = json_i64_by_keys(
                item,
                &[
                    "public_port",
                    "external_port",
                    "out_port",
                    "src_port",
                    "source_port",
                    "port",
                ],
            );
            let private_port = json_i64_by_keys(
                item,
                &[
                    "private_port",
                    "internal_port",
                    "dst_port",
                    "target_port",
                    "destination_port",
                    "nat_port",
                ],
            );
            let mapping_key = json_string_by_keys(item, &["id", "rule_id"])
                .or_else(|| {
                    Some(format!(
                        "{}:{}:{}",
                        json_string_by_keys(item, &["public_ip", "external_ip"])
                            .unwrap_or_default(),
                        public_port.unwrap_or_default(),
                        private_port.unwrap_or_default()
                    ))
                })
                .unwrap_or_default();
            LazycatPortMappingRecord {
                family: "nat".to_string(),
                mapping_key,
                public_ip: json_string_by_keys(item, &["public_ip", "external_ip", "source_ip"]),
                public_port,
                public_port_end: json_i64_by_keys(
                    item,
                    &["public_port_end", "external_port_end", "source_port_end"],
                ),
                private_ip: json_string_by_keys(
                    item,
                    &["private_ip", "internal_ip", "container_ip", "target_ip"],
                ),
                private_port,
                private_port_end: json_i64_by_keys(
                    item,
                    &["private_port_end", "internal_port_end", "target_port_end"],
                ),
                protocol: json_string_by_keys(item, &["protocol", "type"]),
                status: json_string_by_keys(item, &["status", "state"]),
                description: json_string_by_keys(item, &["description", "remark", "note"]),
                remote_created_at: json_string_by_keys(item, &["created_at", "create_time"])
                    .and_then(|value| normalize_datetime_string(&value)),
                remote_updated_at: json_string_by_keys(item, &["updated_at", "update_time"])
                    .and_then(|value| normalize_datetime_string(&value)),
            }
        })
        .collect()
}

fn json_string_by_keys(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .map(|value| value.to_string())
}

fn json_i64_by_keys(value: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_i64))
}

fn normalize_datetime_string(value: &str) -> Option<String> {
    if let Ok(parsed) = OffsetDateTime::parse(value, &Rfc3339) {
        return parsed.format(&Rfc3339).ok();
    }
    None
}

fn should_retry_insecure(url: &Url, err: &anyhow::Error, allow_invalid_tls: bool) -> bool {
    if !allow_invalid_tls || url.scheme() != "https" {
        return false;
    }
    let text = error_chain_text(err).to_ascii_lowercase();
    text.contains("certificate")
        || text.contains("tls")
        || text.contains("x509")
        || text.contains("unknown issuer")
        || text.contains("hostname")
        || text.contains("notvalidforname")
        || text.contains("invalid dnsname")
}

fn error_chain_text(err: &anyhow::Error) -> String {
    let mut text = err.to_string();
    let mut source = err.source();
    while let Some(next) = source {
        text.push_str(" :: ");
        text.push_str(&next.to_string());
        source = next.source();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_login_token() {
        let html = include_str!("../tests/fixtures/lazycat/login.html");
        assert_eq!(
            parse_login_token(html).unwrap(),
            "e80e9bfc52e78fa02f3de22ea20043c3"
        );
    }

    #[test]
    fn parses_clientarea_pages() {
        let first = include_str!("../tests/fixtures/lazycat/clientarea-page1.html");
        let second = include_str!("../tests/fixtures/lazycat/clientarea-page2.html");
        let parsed_first = parse_clientarea_page(first).unwrap();
        let parsed_second = parse_clientarea_page(second).unwrap();
        assert_eq!(parsed_first.total_pages, 2);
        assert_eq!(parsed_second.total_pages, 2);
        assert!(parsed_first.service_ids.contains(&2312));
        assert!(parsed_first.service_ids.contains(&3875));
        assert!(parsed_second.service_ids.contains(&5568));
        assert!(parsed_second.service_ids.contains(&5845));
    }

    #[test]
    fn parses_host_detail() {
        let raw = include_str!("../tests/fixtures/lazycat/host-detail-2312.json");
        let json: Value = serde_json::from_str(raw).unwrap();
        let parsed = parse_host_detail(2312, &json, "2026-03-19T14:00:00Z").unwrap();
        assert_eq!(parsed.site.service_name, "港湾 Transit Mini");
        assert_eq!(parsed.site.service_code, "srvQ8L2M5R1P9K");
        assert_eq!(
            parsed.site.primary_address.as_deref(),
            Some("edge-node-24.example.net")
        );
        assert_eq!(parsed.site.billing_cycle.as_deref(), Some("monthly"));
        assert_eq!(parsed.site.renew_price.as_deref(), Some("¥9.34元/月付"));
        assert_eq!(parsed.site.first_price.as_deref(), Some("¥9.34元"));
        assert_eq!(parsed.site.panel_kind.as_deref(), Some("container"));
    }

    #[test]
    fn parses_renew_price() {
        let html = include_str!("../tests/fixtures/lazycat/renew-2312.html");
        assert_eq!(parse_renew_price(html).as_deref(), Some("¥9.34元/月付"));
    }

    #[test]
    fn parses_panel_snapshot() {
        let html = include_str!("../tests/fixtures/lazycat/panel-info-2312.html");
        let panel = parse_container_panel_snapshot(html).unwrap();
        assert_eq!(
            panel.panel_url,
            "https://edge-node-24.example.net:8443/container/dashboard?hash=8d1f0c27b4a9e3f2"
        );
        assert_eq!(panel.panel_hash, "8d1f0c27b4a9e3f2");
    }

    #[test]
    fn parses_panel_detail_and_ports() {
        let info: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/lazycat/panel-container-info.json"
        ))
        .unwrap();
        let detail = parse_panel_detail(&info).unwrap();
        assert_eq!(detail.traffic_reset_day, Some(11));
        assert_eq!(
            detail.traffic_display.as_deref(),
            Some("700.22 GB / 800 GB")
        );

        let ipv4: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/lazycat/panel-port-mapping-v4.json"
        ))
        .unwrap();
        let ipv6: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/lazycat/panel-port-mapping-v6.json"
        ))
        .unwrap();
        let mappings_v4 = parse_container_port_mappings(&ipv4, "v4");
        let mappings_v6 = parse_container_port_mappings(&ipv6, "v6");
        assert_eq!(mappings_v4.len(), 2);
        assert_eq!(mappings_v4[0].public_port, Some(52222));
        assert!(mappings_v6.is_empty());
    }

    #[test]
    fn nat_proxy_500_returns_error() {
        let json: Value =
            serde_json::from_str(include_str!("../tests/fixtures/lazycat/natlist-error.json"))
                .unwrap();
        let err = ensure_nat_response_ok(&json).unwrap_err();
        assert!(err.to_string().contains("连接服务器失败"));
    }

    #[test]
    fn invalid_tls_retry_only_for_https_and_tls_like_errors() {
        let url = Url::parse("https://example.com:8443/api/container/info").unwrap();
        let err = anyhow!("certificate verify failed: self signed certificate");
        assert!(should_retry_insecure(&url, &err, true));
        assert!(!should_retry_insecure(&url, &err, false));
        let http_url = Url::parse("http://example.com/api/container/info").unwrap();
        assert!(!should_retry_insecure(&http_url, &err, true));
    }

    #[test]
    fn sync_due_uses_interval_minutes_and_invalid_timestamps_are_due() {
        let now = OffsetDateTime::parse("2026-03-19T15:00:00Z", &Rfc3339).unwrap();
        assert!(sync_due(None, now, 5));
        assert!(sync_due(Some("invalid"), now, 5));
        assert!(!sync_due(Some("2026-03-19T14:56:00Z"), now, 5));
        assert!(sync_due(Some("2026-03-19T14:55:00Z"), now, 5));
        assert!(sync_due(Some("2026-03-19T14:54:59Z"), now, 5));
        assert!(!sync_due(Some("2026-03-19T14:59:30Z"), now, 0));
        assert!(sync_due(Some("2026-03-19T14:58:59Z"), now, 0));
    }

    #[test]
    fn panel_sync_summary_preserves_previous_success_when_all_attempts_fail() {
        let mut account = LazycatAccountRow {
            user_id: "u_1".to_string(),
            email: "user@example.com".to_string(),
            password: "secret".to_string(),
            cookies_json: None,
            state: "syncing".to_string(),
            last_error: None,
            last_authenticated_at: None,
            last_site_sync_at: Some("2026-03-19T14:00:00Z".to_string()),
            last_panel_sync_at: Some("2026-03-18T14:00:00Z".to_string()),
            created_at: "2026-03-19T13:00:00Z".to_string(),
            updated_at: "2026-03-19T13:00:00Z".to_string(),
        };

        apply_panel_sync_summary(
            &mut account,
            PanelSyncSummary {
                attempted: 2,
                successful: 0,
                failed: 2,
            },
            "2026-03-19T15:00:00Z",
        );

        assert_eq!(account.state, "error");
        assert_eq!(
            account.last_panel_sync_at.as_deref(),
            Some("2026-03-18T14:00:00Z")
        );
        assert!(account
            .last_error
            .as_deref()
            .is_some_and(|message| message.contains("全部失败")));
    }

    #[test]
    fn panel_sync_summary_marks_partial_success_without_losing_timestamp() {
        let mut account = LazycatAccountRow {
            user_id: "u_1".to_string(),
            email: "user@example.com".to_string(),
            password: "secret".to_string(),
            cookies_json: None,
            state: "syncing".to_string(),
            last_error: Some("old".to_string()),
            last_authenticated_at: None,
            last_site_sync_at: Some("2026-03-19T14:00:00Z".to_string()),
            last_panel_sync_at: None,
            created_at: "2026-03-19T13:00:00Z".to_string(),
            updated_at: "2026-03-19T13:00:00Z".to_string(),
        };

        apply_panel_sync_summary(
            &mut account,
            PanelSyncSummary {
                attempted: 3,
                successful: 2,
                failed: 1,
            },
            "2026-03-19T15:00:00Z",
        );

        assert_eq!(account.state, "ready");
        assert_eq!(
            account.last_panel_sync_at.as_deref(),
            Some("2026-03-19T15:00:00Z")
        );
        assert!(account
            .last_error
            .as_deref()
            .is_some_and(|message| message.contains("部分面板同步失败")));
    }

    #[test]
    fn stale_port_mapping_families_follow_panel_kind() {
        assert_eq!(stale_port_mapping_families(Some("container")), &["nat"]);
        assert_eq!(stale_port_mapping_families(Some("nat")), &["v4", "v6"]);
        assert_eq!(stale_port_mapping_families(None), &["v4", "v6", "nat"]);
    }
}
