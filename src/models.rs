use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: ErrorInfo,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorInfo {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserView {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Country {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Region {
    pub id: String,
    pub country_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Spec {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Money {
    pub amount: f64,
    pub currency: String,
    pub period: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Inventory {
    pub status: String,
    pub quantity: i64,
    pub checked_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigView {
    pub id: String,
    pub country_id: String,
    pub region_id: Option<String>,
    pub name: String,
    pub specs: Vec<Spec>,
    pub price: Money,
    pub inventory: Inventory,
    pub digest: String,
    pub lifecycle: ConfigLifecycleView,
    pub monitor_supported: bool,
    pub monitor_enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigLifecycleView {
    pub state: String,
    pub listed_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delisted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogView {
    pub countries: Vec<Country>,
    pub regions: Vec<Region>,
    pub configs: Vec<ConfigView>,
    pub fetched_at: String,
    pub source: CatalogSource,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogSource {
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitoringPollView {
    pub interval_seconds: i64,
    pub jitter_pct: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitoringView {
    pub enabled_config_ids: Vec<String>,
    pub poll: MonitoringPollView,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub poll: SettingsPollView,
    pub site_base_url: Option<String>,
    pub catalog_refresh: SettingsCatalogRefreshView,
    pub monitoring_events: SettingsMonitoringEventsView,
    pub notifications: SettingsNotificationsView,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsCatalogRefreshView {
    pub auto_interval_hours: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsMonitoringEventsView {
    pub listed_enabled: bool,
    pub delisted_enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPollView {
    pub interval_minutes: i64,
    pub jitter_pct: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsNotificationsView {
    pub telegram: TelegramSettingsView,
    pub web_push: WebPushSettingsView,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TelegramSettingsView {
    pub enabled: bool,
    pub configured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebPushSettingsView {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vapid_public_key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppMetaView {
    pub effective_version: String,
    pub web_dist_build_id: String,
    pub repo_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapResponse {
    pub user: UserView,
    pub app: AppMetaView,
    pub catalog: CatalogView,
    pub monitoring: MonitoringView,
    pub settings: SettingsView,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LatestReleaseView {
    pub tag: String,
    pub version: String,
    pub html_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResponse {
    pub current_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest: Option<LatestReleaseView>,
    pub update_available: bool,
    pub checked_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitoringToggleRequest {
    pub enabled: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitoringToggleResponse {
    pub config_id: String,
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsUpdateRequest {
    pub poll: SettingsPollUpdate,
    pub site_base_url: Option<String>,
    pub notifications: SettingsNotificationsUpdate,
    #[serde(default)]
    pub catalog_refresh: Option<SettingsCatalogRefreshUpdate>,
    #[serde(default)]
    pub monitoring_events: Option<SettingsMonitoringEventsUpdate>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsCatalogRefreshUpdate {
    pub auto_interval_hours: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsMonitoringEventsUpdate {
    pub listed_enabled: bool,
    pub delisted_enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPollUpdate {
    pub interval_minutes: i64,
    pub jitter_pct: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsNotificationsUpdate {
    pub telegram: TelegramSettingsUpdate,
    pub web_push: WebPushSettingsUpdate,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelegramSettingsUpdate {
    pub enabled: bool,
    pub bot_token: Option<String>,
    pub target: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebPushSettingsUpdate {
    pub enabled: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductsResponse {
    pub configs: Vec<ConfigView>,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogRefreshStatus {
    pub job_id: String,
    pub state: String,
    pub trigger: String,
    pub done: i64,
    pub total: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    pub started_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<CatalogRefreshCurrent>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogRefreshCurrent {
    pub url_key: String,
    pub url: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryHistoryRequest {
    pub config_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryHistoryResponse {
    pub window: InventoryHistoryWindow,
    pub series: Vec<InventoryHistorySeries>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryHistoryWindow {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryHistorySeries {
    pub config_id: String,
    pub points: Vec<InventoryHistoryPoint>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryHistoryPoint {
    pub ts_minute: String,
    pub quantity: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshStatusResponse {
    pub state: String,
    pub done: i64,
    pub total: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitoringListResponse {
    pub items: Vec<ConfigView>,
    pub fetched_at: String,
    pub recent_listed24h: Vec<ConfigView>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntryView {
    pub id: String,
    pub ts: String,
    pub level: String,
    pub scope: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogsResponse {
    pub items: Vec<LogEntryView>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebPushSubscribeRequest {
    pub subscription: WebPushSubscription,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebPushSubscription {
    pub endpoint: String,
    pub keys: WebPushKeys,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebPushKeys {
    pub p256dh: String,
    pub auth: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebPushSubscribeResponse {
    pub subscription_id: String,
}
