use crate::models::{Country, Inventory, Money, Region, RegionNotice, Spec};
use scraper::{ElementRef, Html, Selector};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct CatalogSnapshot {
    pub countries: Vec<Country>,
    pub regions: Vec<Region>,
    pub region_notices: Vec<RegionNotice>,
    pub configs: Vec<ConfigBase>,
    pub fetched_at: String,
    pub source_url: String,
}

#[derive(Debug, Clone)]
pub struct ConfigBase {
    pub id: String,
    pub country_id: String,
    pub region_id: Option<String>,
    pub name: String,
    pub specs: Vec<Spec>,
    pub price: Money,
    pub inventory: Inventory,
    pub digest: String,
    pub monitor_supported: bool,
    pub source_pid: Option<String>,
    pub source_fid: Option<String>,
    pub source_gid: Option<String>,
}

impl CatalogSnapshot {
    pub fn empty(source_url: String) -> Self {
        Self {
            countries: Vec::new(),
            regions: Vec::new(),
            region_notices: Vec::new(),
            configs: Vec::new(),
            fetched_at: now_rfc3339(),
            source_url,
        }
    }

    pub fn to_view(&self, c: &ConfigBase, monitor_enabled: bool) -> crate::models::ConfigView {
        crate::models::ConfigView {
            id: c.id.clone(),
            country_id: c.country_id.clone(),
            region_id: c.region_id.clone(),
            name: c.name.clone(),
            specs: c.specs.clone(),
            price: c.price.clone(),
            inventory: c.inventory.clone(),
            digest: c.digest.clone(),
            lifecycle: crate::models::ConfigLifecycleView {
                state: "active".to_string(),
                listed_at: c.inventory.checked_at.clone(),
                delisted_at: None,
                cleanup_at: None,
            },
            monitor_supported: c.monitor_supported,
            monitor_enabled,
            source_pid: c.source_pid.clone(),
            source_fid: c.source_fid.clone(),
            source_gid: c.source_gid.clone(),
        }
    }
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

#[derive(Clone)]
pub struct UpstreamClient {
    client: reqwest::Client,
    cart_url: String,
    pid_name_cache: Arc<Mutex<HashMap<String, String>>>,
}

#[derive(Debug, Clone)]
pub struct RegionFetchDetailed {
    pub url: String,
    pub http_status: u16,
    pub bytes: i64,
    pub elapsed_ms: i64,
    pub parse_elapsed_ms: i64,
    pub configs: Vec<ConfigBase>,
    pub region_notice: Option<String>,
}

impl UpstreamClient {
    pub fn new(cart_url: String) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent("catnap/0.1 (+https://example.invalid)")
            .build()?;
        Ok(Self {
            client,
            cart_url,
            pid_name_cache: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub async fn fetch_html_raw(&self, url: &str) -> anyhow::Result<String> {
        self.fetch_html(url).await
    }

    pub async fn fetch_catalog(&self) -> anyhow::Result<CatalogSnapshot> {
        let root_html = self.fetch_html(&self.cart_url).await?;
        let countries = parse_countries(&root_html);

        let mut regions = Vec::new();
        let mut region_notices = BTreeMap::<(String, Option<String>), String>::new();
        let mut configs = Vec::new();

        // Keep concurrency low to avoid hammering upstream.
        for c in &countries {
            let fid = &c.id;
            let fid_url = format!("{}?fid={fid}", self.cart_url);
            let fid_html = self.fetch_html(&fid_url).await?;
            let mut fid_regions = parse_regions(fid, &fid_html);
            if fid_regions.is_empty() {
                // Some pages may not have a region selector.
                if let Some(text) = parse_region_notice(&fid_html) {
                    upsert_region_notice(&mut region_notices, fid, None, &text);
                }
                let parsed = parse_configs(fid, None, &fid_html);
                configs.extend(parsed);
            } else {
                regions.append(&mut fid_regions);
            }

            // If we did get regions, fetch each region's configs.
            for r in regions.iter().filter(|r| &r.country_id == fid) {
                let gid = &r.id;
                let gid_url = format!("{}?fid={fid}&gid={gid}", self.cart_url);
                let gid_html = self.fetch_html(&gid_url).await?;
                if let Some(text) = parse_region_notice(&gid_html) {
                    upsert_region_notice(&mut region_notices, fid, Some(gid), &text);
                }
                let parsed = parse_configs(fid, Some(gid), &gid_html);
                configs.extend(parsed);
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            }

            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }

        let fetched_at = now_rfc3339();
        for c in &mut configs {
            c.inventory.checked_at = fetched_at.clone();
        }
        self.resolve_missing_source_pids(&mut configs).await;

        Ok(CatalogSnapshot {
            countries,
            regions,
            region_notices: region_notices
                .into_iter()
                .map(|((country_id, region_id), text)| RegionNotice {
                    country_id,
                    region_id,
                    text,
                })
                .collect(),
            configs,
            fetched_at,
            source_url: self.cart_url.clone(),
        })
    }

    pub async fn fetch_region_configs(
        &self,
        fid: &str,
        gid: Option<&str>,
    ) -> anyhow::Result<Vec<ConfigBase>> {
        let url = if let Some(gid) = gid {
            format!("{}?fid={fid}&gid={gid}", self.cart_url)
        } else {
            format!("{}?fid={fid}", self.cart_url)
        };
        let html = self.fetch_html(&url).await?;
        Ok(parse_configs(fid, gid, &html))
    }

    pub async fn fetch_region_configs_detailed(
        &self,
        fid: &str,
        gid: Option<&str>,
    ) -> anyhow::Result<RegionFetchDetailed> {
        use std::time::Instant;

        let url = if let Some(gid) = gid {
            format!("{}?fid={fid}&gid={gid}", self.cart_url)
        } else {
            format!("{}?fid={fid}", self.cart_url)
        };

        let start = Instant::now();
        let res = self.client.get(&url).send().await?;
        let status = res.status();
        let http_status = status.as_u16();
        if !status.is_success() {
            anyhow::bail!("upstream http {status} for {url}");
        }
        let html = res.text().await?;
        let elapsed_ms = start.elapsed().as_millis() as i64;
        let bytes = html.len() as i64;

        let parse_start = Instant::now();
        let configs = parse_configs(fid, gid, &html);
        let parse_elapsed_ms = parse_start.elapsed().as_millis() as i64;
        let region_notice = parse_region_notice(&html);

        if configs.is_empty() {
            anyhow::bail!("upstream parse produced 0 configs for {url}");
        }

        Ok(RegionFetchDetailed {
            url,
            http_status,
            bytes,
            elapsed_ms,
            parse_elapsed_ms,
            configs,
            region_notice,
        })
    }

    async fn fetch_html(&self, url: &str) -> anyhow::Result<String> {
        let res = self.client.get(url).send().await?;
        let status = res.status();
        if !status.is_success() {
            anyhow::bail!("upstream http {status} for {url}");
        }
        Ok(res.text().await?)
    }

    async fn resolve_missing_source_pids(&self, configs: &mut [ConfigBase]) {
        let mut unresolved_by_name: HashMap<String, Vec<usize>> = HashMap::new();
        let mut max_known_pid: u32 = 0;
        for (idx, cfg) in configs.iter().enumerate() {
            if let Some(pid) = cfg
                .source_pid
                .as_deref()
                .and_then(|v| v.parse::<u32>().ok())
            {
                max_known_pid = max_known_pid.max(pid);
                continue;
            }
            unresolved_by_name
                .entry(cfg.name.clone())
                .or_default()
                .push(idx);
        }
        if unresolved_by_name.is_empty() {
            return;
        }
        for (name, idxs) in &unresolved_by_name {
            if let Some(pid) = known_pid_fallback(name) {
                for idx in idxs {
                    configs[*idx].source_pid = Some(pid.to_string());
                }
            }
        }
        unresolved_by_name
            .retain(|_, idxs| idxs.iter().any(|idx| configs[*idx].source_pid.is_none()));
        if unresolved_by_name.is_empty() {
            return;
        }
        {
            let cache = self.pid_name_cache.lock().await;
            for (name, idxs) in &unresolved_by_name {
                if let Some(pid) = cache.get(name) {
                    for idx in idxs {
                        configs[*idx].source_pid = Some(pid.clone());
                    }
                }
            }
        }
        unresolved_by_name
            .retain(|_, idxs| idxs.iter().any(|idx| configs[*idx].source_pid.is_none()));
        if unresolved_by_name.is_empty() {
            return;
        }

        // Probe configureproduct pages to recover pids hidden on sold-out cards.
        // Keep this bounded and conservative to avoid overloading upstream.
        let probe_max = max_known_pid.max(200);
        let mut recovered: HashMap<String, String> = HashMap::new();
        for pid in 1..=probe_max {
            if unresolved_by_name
                .keys()
                .all(|name| recovered.contains_key(name))
            {
                break;
            }
            let url = format!("{}?action=configureproduct&pid={pid}", self.cart_url);
            let Ok(html) = self.fetch_html(&url).await else {
                continue;
            };
            let Some(title) = parse_configureproduct_title(&html) else {
                continue;
            };
            if unresolved_by_name.contains_key(&title) {
                recovered.entry(title).or_insert_with(|| pid.to_string());
            }
            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
        }

        for (name, idxs) in unresolved_by_name {
            if let Some(pid) = recovered.get(&name) {
                for idx in idxs {
                    configs[idx].source_pid = Some(pid.clone());
                }
            }
        }
        if !recovered.is_empty() {
            let mut cache = self.pid_name_cache.lock().await;
            for (name, pid) in recovered {
                cache.insert(name, pid);
            }
        }
    }
}

fn extract_query_number(s: &str, key: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let key_bytes = key.as_bytes();
    if key_bytes.is_empty() {
        return None;
    }

    let mut start = 0usize;
    while start < bytes.len() {
        let rel = s[start..].find(key)?;
        let idx = start + rel;
        let after = idx + key_bytes.len();

        // Match whole token so keys like `pid` don't match `rapid`.
        let left_ok =
            idx == 0 || !(bytes[idx - 1].is_ascii_alphanumeric() || bytes[idx - 1] == b'_');
        let right_ok =
            after >= bytes.len() || !(bytes[after].is_ascii_alphanumeric() || bytes[after] == b'_');
        if !(left_ok && right_ok) {
            start = after;
            continue;
        }

        let mut i = after;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            start = after;
            continue;
        }
        i += 1;
        while i < bytes.len()
            && (bytes[i].is_ascii_whitespace() || bytes[i] == b'\'' || bytes[i] == b'"')
        {
            i += 1;
        }
        let begin = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i > begin {
            return Some(s[begin..i].to_string());
        }
        start = after;
    }
    None
}

pub fn parse_countries(html: &str) -> Vec<Country> {
    let doc = Html::parse_document(html);
    let item = Selector::parse(".firstgroup_item").unwrap();
    let title = Selector::parse(".yy-bth-text-a").unwrap();
    let mut out = Vec::new();

    for el in doc.select(&item) {
        let onclick = el.value().attr("onclick").unwrap_or_default();
        let Some(fid) = extract_query_number(onclick, "fid") else {
            continue;
        };
        let name = el
            .select(&title)
            .next()
            .map(|t| normalize_text(&t.text().collect::<String>()))
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| fid.clone());
        out.push(Country { id: fid, name });
    }
    out
}

pub fn parse_regions(fid: &str, html: &str) -> Vec<Region> {
    let doc = Html::parse_document(html);
    let item = Selector::parse(".secondgroup_item").unwrap();
    let a_title = Selector::parse(".yy-bth-text-a").unwrap();
    let a_sub = Selector::parse(".yy-bth-text-b").unwrap();
    let mut out = Vec::new();

    for el in doc.select(&item) {
        let onclick = el.value().attr("onclick").unwrap_or_default();
        let Some(gid) = extract_query_number(onclick, "gid") else {
            continue;
        };
        let name = el
            .select(&a_title)
            .next()
            .map(|t| normalize_text(&t.text().collect::<String>()))
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| gid.clone());
        let location_name = el
            .select(&a_sub)
            .next()
            .map(|t| normalize_text(&t.text().collect::<String>()))
            .filter(|v| !v.is_empty());
        out.push(Region {
            id: gid,
            country_id: fid.to_string(),
            name,
            location_name,
        });
    }
    out
}

pub fn parse_region_notice(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let area = Selector::parse(".secondgroup_box .secondgroup_box_area.yy-dtjbt-text").unwrap();

    let mut notice = None;
    for el in doc.select(&area) {
        let text = normalize_text(&el.text().collect::<String>());
        if text.is_empty() || is_region_title_only(&text) {
            continue;
        }
        notice = Some(text);
    }
    notice
}

pub fn parse_configs(fid: &str, gid: Option<&str>, html: &str) -> Vec<ConfigBase> {
    let doc = Html::parse_document(html);
    let card = Selector::parse(".card.cartitem").unwrap();
    let h4 = Selector::parse("h4").unwrap();
    let specs_block = Selector::parse(".card-text").unwrap();
    let a_price = Selector::parse("a.cart-num").unwrap();
    let price_block = Selector::parse(".text-right").unwrap();
    let p_tag = Selector::parse("p").unwrap();
    let li_tag = Selector::parse("li").unwrap();
    let a_tag = Selector::parse("a").unwrap();

    let mut out = Vec::new();
    for el in doc.select(&card) {
        let name = el
            .select(&h4)
            .next()
            .map(|t| normalize_text(&t.text().collect::<String>()))
            .filter(|v| !v.is_empty());
        let Some(name) = name else { continue };

        let mut specs = Vec::new();
        if let Some(spec_root) = el.select(&specs_block).next() {
            for p in spec_root.select(&p_tag) {
                if let Some((k, v)) = split_kv(&normalize_text(&p.text().collect::<String>())) {
                    specs.push(Spec { key: k, value: v });
                }
            }
            for li in spec_root.select(&li_tag) {
                if let Some((k, v)) = split_kv(&normalize_text(&li.text().collect::<String>())) {
                    specs.push(Spec { key: k, value: v });
                }
            }
        }

        let amount = el
            .select(&a_price)
            .next()
            .and_then(|t| {
                normalize_text(&t.text().collect::<String>())
                    .parse::<f64>()
                    .ok()
            })
            .unwrap_or(0.0);
        let price_line_text = el
            .select(&price_block)
            .find(|node| node.select(&a_price).next().is_some())
            .map(|v| normalize_text(&v.text().collect::<String>()))
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| extract_price_line_from_price_anchor(&el, &a_price));
        let price = Money {
            amount,
            currency: "CNY".to_string(),
            period: detect_price_period(&price_line_text, &name).to_string(),
        };

        let mut inventory_quantity: Option<i64> = None;
        for p in el.select(&p_tag) {
            let text = normalize_text(&p.text().collect::<String>());
            if text.contains("库存") {
                inventory_quantity = extract_first_int(&text);
                break;
            }
        }
        let monitor_supported = fid != "2";
        let (status, quantity) = if !monitor_supported {
            ("available".to_string(), 1)
        } else if let Some(q) = inventory_quantity {
            let status = if q > 0 { "available" } else { "unavailable" };
            (status.to_string(), q)
        } else {
            ("unknown".to_string(), 0)
        };

        let mut source_pid: Option<String> = None;
        for a in el.select(&a_tag) {
            let href = a.value().attr("href").unwrap_or_default();
            source_pid = extract_query_number(href, "pid")
                .or_else(|| {
                    extract_query_number(a.value().attr("onclick").unwrap_or_default(), "pid")
                })
                .or_else(|| {
                    extract_query_number(a.value().attr("data-pid").unwrap_or_default(), "pid")
                });
            if source_pid.is_some() {
                break;
            }
        }

        let id = make_config_id(fid, gid, source_pid.as_deref(), &name);
        let digest = compute_digest(&name, &specs, &price);

        out.push(ConfigBase {
            id,
            country_id: fid.to_string(),
            region_id: gid.map(|v| v.to_string()),
            name,
            specs,
            price,
            inventory: Inventory {
                status,
                quantity,
                checked_at: now_rfc3339(),
            },
            digest,
            monitor_supported,
            source_pid,
            source_fid: Some(fid.to_string()),
            source_gid: gid.map(|v| v.to_string()),
        });
    }
    out
}

fn compute_digest(name: &str, specs: &[Spec], price: &Money) -> String {
    let mut hasher = Sha256::new();
    hasher.update(name.as_bytes());
    hasher.update(b"\n");
    for s in specs {
        hasher.update(s.key.as_bytes());
        hasher.update(b"=");
        hasher.update(s.value.as_bytes());
        hasher.update(b"\n");
    }
    hasher.update(price.amount.to_string().as_bytes());
    hasher.update(b"\n");
    hasher.update(price.currency.as_bytes());
    hasher.update(b"\n");
    hasher.update(price.period.as_bytes());
    hex::encode(hasher.finalize())
}

fn make_config_id(fid: &str, gid: Option<&str>, pid: Option<&str>, name: &str) -> String {
    let gid_part = gid.unwrap_or("0");
    if let Some(pid) = pid {
        return format!("lc:{fid}:{gid_part}:{pid}");
    }
    let mut hasher = Sha256::new();
    hasher.update(name.as_bytes());
    let short = &hex::encode(hasher.finalize())[0..12];
    format!("lc:{fid}:{gid_part}:{short}")
}

fn extract_first_int(s: &str) -> Option<i64> {
    let mut buf = String::new();
    for ch in s.chars() {
        if ch.is_ascii_digit() {
            buf.push(ch);
        } else if !buf.is_empty() {
            break;
        }
    }
    buf.parse::<i64>().ok()
}

fn normalize_text(s: &str) -> String {
    s.replace('\u{00A0}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn is_region_title_only(s: &str) -> bool {
    let compact = s
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .filter(|ch| !matches!(ch, '|' | '｜'))
        .collect::<String>();

    let normalized = compact
        .trim_start_matches('📍')
        .trim_start_matches(':')
        .trim_start_matches('：')
        .trim();
    normalized == "可用区域" || normalized == "可用區域"
}

fn upsert_region_notice(
    notices: &mut BTreeMap<(String, Option<String>), String>,
    country_id: &str,
    region_id: Option<&str>,
    text: &str,
) {
    notices.insert(
        (
            country_id.to_string(),
            region_id.map(std::string::ToString::to_string),
        ),
        text.to_string(),
    );
}

fn detect_price_period(price_text: &str, name: &str) -> &'static str {
    detect_period_from_price_text(price_text)
        .or_else(|| detect_period_from_name(name))
        .unwrap_or("month")
}

fn detect_period_from_price_text(raw: &str) -> Option<&'static str> {
    let compact: String = raw.chars().filter(|ch| !ch.is_whitespace()).collect();
    if compact.is_empty() {
        return None;
    }
    let lower = compact.to_ascii_lowercase();

    // Explicit bill-cycle markers in price rows are authoritative. If both exist,
    // choose the earliest marker in the price text instead of fixed year-first.
    let year_explicit_pos = first_match_pos(&compact, &["/年", "／年"])
        .into_iter()
        .chain(first_match_pos(&lower, &["/year", "/yr"]))
        .min();
    let month_explicit_pos = first_match_pos(&compact, &["/月", "／月"])
        .into_iter()
        .chain(first_match_pos(&lower, &["/month", "/mo"]))
        .min();
    match (month_explicit_pos, year_explicit_pos) {
        (Some(month_pos), Some(year_pos)) => {
            return Some(if month_pos <= year_pos {
                "month"
            } else {
                "year"
            });
        }
        (Some(_), None) => return Some("month"),
        (None, Some(_)) => return Some("year"),
        (None, None) => {}
    }

    let year_keyword_pos = first_match_pos(&compact, &["年付", "按年", "年缴", "年费"])
        .into_iter()
        .chain(first_match_pos(&lower, &["annual", "yearly"]))
        .min();
    let month_keyword_pos = first_match_pos(&compact, &["月付", "按月", "月缴", "月费"])
        .into_iter()
        .chain(first_match_pos(&lower, &["monthly"]))
        .min();
    match (month_keyword_pos, year_keyword_pos) {
        (Some(month_pos), Some(year_pos)) => Some(if month_pos <= year_pos {
            "month"
        } else {
            "year"
        }),
        (Some(_), None) => Some("month"),
        (None, Some(_)) => Some("year"),
        (None, None) => None,
    }
}

fn detect_period_from_name(name: &str) -> Option<&'static str> {
    let compact: String = name.chars().filter(|ch| !ch.is_whitespace()).collect();
    if compact.is_empty() {
        return None;
    }
    let lower = compact.to_ascii_lowercase();

    if compact.contains("年付")
        || compact.contains("年缴")
        || compact.contains("按年")
        || compact.contains("包年")
        || lower.contains("annual")
        || lower.contains("yearly")
    {
        return Some("year");
    }
    if compact.contains("月付")
        || compact.contains("月缴")
        || compact.contains("按月")
        || lower.contains("monthly")
    {
        return Some("month");
    }
    None
}

fn first_match_pos(text: &str, patterns: &[&str]) -> Option<usize> {
    patterns
        .iter()
        .filter_map(|pattern| text.find(pattern))
        .min()
}

fn extract_price_line_from_price_anchor(card: &ElementRef<'_>, price_anchor: &Selector) -> String {
    let Some(anchor) = card.select(price_anchor).next() else {
        return String::new();
    };
    if let Some(parent) = anchor.parent().and_then(ElementRef::wrap) {
        let parent_text = normalize_text(&parent.text().collect::<String>());
        if !parent_text.is_empty() {
            return parent_text;
        }
    }
    normalize_text(&anchor.text().collect::<String>())
}

fn split_kv(s: &str) -> Option<(String, String)> {
    let (key, value) = s.split_once('：').or_else(|| s.split_once(':'))?;
    let key = key.trim();
    let value = value.trim();
    if key.is_empty() || value.is_empty() {
        None
    } else {
        Some((key.to_string(), value.to_string()))
    }
}

fn parse_configureproduct_title(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let h4 = Selector::parse("h4").ok()?;
    for node in doc.select(&h4) {
        let text = normalize_text(&node.text().collect::<String>());
        if text.is_empty() || text == "产品配置" || text == "订单汇总：" || text == "订单汇总:"
        {
            continue;
        }
        return Some(text);
    }
    None
}

fn known_pid_fallback(name: &str) -> Option<&'static str> {
    match name {
        "新加坡优化 Mini" => Some("48"),
        "新加坡优化 Basic" => Some("49"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_countries() {
        let html = include_str!("../tests/fixtures/cart-root.html");
        let countries = parse_countries(html);
        assert!(countries.iter().any(|c| c.id == "2"));
        assert!(countries.iter().any(|c| c.id == "7"));
    }

    #[test]
    fn parses_regions() {
        let html = include_str!("../tests/fixtures/cart-fid-2.html");
        let regions = parse_regions("2", html);
        assert!(regions.iter().any(|r| r.id == "56"));
    }

    #[test]
    fn parses_region_notice_skipping_region_title() {
        let html = r#"
        <html><body>
          <div class="secondgroup_box mb-2 flex-column p-2">
            <div class="secondgroup_box_area fs-22 ml-3 mt-2 pl-1 w-100 yy-dtjbt-text">
              <span class="yy-bl"></span> 📍可用区域
            </div>
          </div>
          <div class="secondgroup_box mb-2">
            <div class="secondgroup_box_area mr-2 fs-16 yy-dtjbt-text">
              <span class="yy-bl"></span>
              <span class="fs-18">
                台湾苗栗Hinet 高质量IP 三网直连 动态IP 带IPV6
                <p>禁止滥用！发包、机场、扫描等滥用行为！</p>
              </span>
            </div>
          </div>
        </body></html>
        "#;
        let notice = parse_region_notice(html).expect("notice should be present");
        assert!(notice.contains("台湾苗栗Hinet"));
        assert!(notice.contains("禁止滥用"));
    }

    #[test]
    fn parse_region_notice_returns_none_when_only_title_exists() {
        let html = r#"
        <html><body>
          <div class="secondgroup_box mb-2 flex-column p-2">
            <div class="secondgroup_box_area fs-22 ml-3 mt-2 pl-1 w-100 yy-dtjbt-text">
              📍可用区域
            </div>
          </div>
        </body></html>
        "#;
        assert_eq!(parse_region_notice(html), None);
    }

    #[test]
    fn parses_configs_with_inventory() {
        let html = include_str!("../tests/fixtures/cart-fid-7.html");
        let configs = parse_configs("7", Some("40"), html);
        let cfg = configs.iter().find(|c| c.name.contains("Basic")).unwrap();
        assert!(cfg.monitor_supported);
        assert!(cfg.inventory.quantity >= 0);
        assert_eq!(cfg.price.period, "month");
    }

    #[test]
    fn parses_configs_detects_year_period_from_price_line() {
        let html = include_str!("../tests/fixtures/cart-fid-11-year.html");
        let configs = parse_configs("11", Some("81"), html);
        assert!(!configs.is_empty());
        assert_eq!(configs[0].price.period, "year");
    }

    #[test]
    fn parse_configs_fallbacks_to_name_period_when_price_line_has_no_period() {
        let html = r#"
        <html><body>
          <div class="card cartitem shadow w-100">
            <div class="card-body">
              <h4>芬兰特惠年付 Mini</h4>
            </div>
            <div class="text-right">
              ¥ <a class="cart-num DINCondensed-Bold">4.99</a> 元
            </div>
            <div class="card-footer">
              <a href="/cart?action=configureproduct&pid=188">立即购买</a>
            </div>
          </div>
        </body></html>
        "#;
        let configs = parse_configs("11", Some("81"), html);
        assert!(!configs.is_empty());
        assert_eq!(configs[0].price.period, "year");
    }

    #[test]
    fn parse_configs_prefers_price_period_over_name_keyword() {
        let html = r#"
        <html><body>
          <div class="card cartitem shadow w-100">
            <div class="card-body">
              <h4>芬兰特惠年付 Mini</h4>
            </div>
            <div class="text-right">
              ¥ <a class="cart-num DINCondensed-Bold">4.99</a> 元 / 月
            </div>
            <div class="card-footer">
              <a href="/cart?action=configureproduct&pid=188">立即购买</a>
            </div>
          </div>
        </body></html>
        "#;
        let configs = parse_configs("11", Some("81"), html);
        assert!(!configs.is_empty());
        assert_eq!(configs[0].price.period, "month");
    }

    #[test]
    fn parse_configs_ignores_non_price_period_words_when_price_block_is_missing() {
        let html = r#"
        <html><body>
          <div class="card cartitem shadow w-100">
            <div class="card-body">
              <h4>芬兰特惠 Mini</h4>
              <div class="card-text">
                <p>备份：每年一次</p>
              </div>
            </div>
            <div class="price-row">
              ¥ <a class="cart-num DINCondensed-Bold">4.99</a> 元
            </div>
            <div class="card-footer">
              <a href="/cart?action=configureproduct&pid=188">立即购买</a>
            </div>
          </div>
        </body></html>
        "#;
        let configs = parse_configs("11", Some("81"), html);
        assert!(!configs.is_empty());
        assert_eq!(configs[0].price.period, "month");
    }

    #[test]
    fn parse_configs_prefers_explicit_month_over_yearly_promo_text() {
        let html = r#"
        <html><body>
          <div class="card cartitem shadow w-100">
            <div class="card-body">
              <h4>芬兰特惠 Mini</h4>
            </div>
            <div class="text-right">
              ¥ <a class="cart-num DINCondensed-Bold">4.99</a> 元 / 月（每年可省 20%）
            </div>
            <div class="card-footer">
              <a href="/cart?action=configureproduct&pid=188">立即购买</a>
            </div>
          </div>
        </body></html>
        "#;
        let configs = parse_configs("11", Some("81"), html);
        assert!(!configs.is_empty());
        assert_eq!(configs[0].price.period, "month");
    }

    #[test]
    fn parse_configs_uses_earliest_explicit_period_marker_when_both_exist() {
        let html = r#"
        <html><body>
          <div class="card cartitem shadow w-100">
            <div class="card-body">
              <h4>芬兰特惠 Mini</h4>
            </div>
            <div class="text-right">
              ¥ <a class="cart-num DINCondensed-Bold">4.99</a> 元 / 月（折合 ¥59.88 / 年）
            </div>
            <div class="card-footer">
              <a href="/cart?action=configureproduct&pid=188">立即购买</a>
            </div>
          </div>
        </body></html>
        "#;
        let configs = parse_configs("11", Some("81"), html);
        assert!(!configs.is_empty());
        assert_eq!(configs[0].price.period, "month");
    }

    #[test]
    fn parse_configs_detects_year_from_price_anchor_parent_without_text_right() {
        let html = r#"
        <html><body>
          <div class="card cartitem shadow w-100">
            <div class="card-body">
              <h4>芬兰特惠 Mini</h4>
            </div>
            <div class="price-row">
              ¥ <a class="cart-num DINCondensed-Bold">59.88</a> 元 / 年
            </div>
            <div class="card-footer">
              <a href="/cart?action=configureproduct&pid=188">立即购买</a>
            </div>
          </div>
        </body></html>
        "#;
        let configs = parse_configs("11", Some("81"), html);
        assert!(!configs.is_empty());
        assert_eq!(configs[0].price.period, "year");
    }

    #[test]
    fn parse_configs_does_not_use_yearly_promo_text_without_billing_marker() {
        let html = r#"
        <html><body>
          <div class="card cartitem shadow w-100">
            <div class="card-body">
              <h4>芬兰特惠 Mini</h4>
            </div>
            <div class="price-row">
              ¥ <a class="cart-num DINCondensed-Bold">4.99</a> 元（每年可省 20%）
            </div>
            <div class="card-footer">
              <a href="/cart?action=configureproduct&pid=188">立即购买</a>
            </div>
          </div>
        </body></html>
        "#;
        let configs = parse_configs("11", Some("81"), html);
        assert!(!configs.is_empty());
        assert_eq!(configs[0].price.period, "month");
    }

    #[test]
    fn parse_configs_uses_text_right_block_that_contains_price_anchor() {
        let html = r#"
        <html><body>
          <div class="card cartitem shadow w-100">
            <div class="card-body">
              <h4>芬兰特惠 Mini</h4>
            </div>
            <div class="text-right">促销：按年折算更划算 / 年</div>
            <div class="text-right">
              ¥ <a class="cart-num DINCondensed-Bold">4.99</a> 元 / 月
            </div>
            <div class="card-footer">
              <a href="/cart?action=configureproduct&pid=188">立即购买</a>
            </div>
          </div>
        </body></html>
        "#;
        let configs = parse_configs("11", Some("81"), html);
        assert!(!configs.is_empty());
        assert_eq!(configs[0].price.period, "month");
    }

    #[test]
    fn cloud_server_configs_disable_monitoring() {
        let html = include_str!("../tests/fixtures/cart-fid-2-gid-56.html");
        let configs = parse_configs("2", Some("56"), html);
        assert!(!configs.is_empty());
        assert!(!configs[0].monitor_supported);
        assert_eq!(configs[0].inventory.quantity, 1);
    }

    #[test]
    fn extract_query_number_supports_reordered_query_and_quoted_values() {
        assert_eq!(
            extract_query_number(
                "javascript:window.location='/cart?pid=321&action=configureproduct'",
                "pid"
            ),
            Some("321".to_string())
        );
        assert_eq!(
            extract_query_number(r#"data-pid="456""#, "pid"),
            Some("456".to_string())
        );
    }

    #[test]
    fn parse_configureproduct_title_ignores_generic_headers() {
        let html = r#"
        <html><body>
          <h4>产品配置</h4>
          <h4>订单汇总：</h4>
          <h4>芬兰特惠年付 Mini</h4>
        </body></html>
        "#;
        assert_eq!(
            parse_configureproduct_title(html),
            Some("芬兰特惠年付 Mini".to_string())
        );
    }
}
