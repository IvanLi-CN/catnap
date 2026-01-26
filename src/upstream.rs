use crate::models::{Country, Inventory, Money, Region, Spec};
use scraper::{Html, Selector};
use sha2::{Digest, Sha256};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

#[derive(Debug, Clone)]
pub struct CatalogSnapshot {
    pub countries: Vec<Country>,
    pub regions: Vec<Region>,
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
            },
            monitor_supported: c.monitor_supported,
            monitor_enabled,
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
}

#[derive(Debug, Clone)]
pub struct RegionFetchDetailed {
    pub url: String,
    pub http_status: u16,
    pub bytes: i64,
    pub elapsed_ms: i64,
    pub parse_elapsed_ms: i64,
    pub configs: Vec<ConfigBase>,
}

impl UpstreamClient {
    pub fn new(cart_url: String) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent("catnap/0.1 (+https://example.invalid)")
            .build()?;
        Ok(Self { client, cart_url })
    }

    pub async fn fetch_html_raw(&self, url: &str) -> anyhow::Result<String> {
        self.fetch_html(url).await
    }

    pub async fn fetch_catalog(&self) -> anyhow::Result<CatalogSnapshot> {
        let root_html = self.fetch_html(&self.cart_url).await?;
        let countries = parse_countries(&root_html);

        let mut regions = Vec::new();
        let mut configs = Vec::new();

        // Keep concurrency low to avoid hammering upstream.
        for c in &countries {
            let fid = &c.id;
            let fid_url = format!("{}?fid={fid}", self.cart_url);
            let fid_html = self.fetch_html(&fid_url).await?;
            let mut fid_regions = parse_regions(fid, &fid_html);
            if fid_regions.is_empty() {
                // Some pages may not have a region selector.
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

        Ok(CatalogSnapshot {
            countries,
            regions,
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
}

fn extract_query_number(s: &str, key: &str) -> Option<String> {
    let idx = s.find(key)?;
    let s = &s[idx + key.len()..];
    let s = s.strip_prefix('=')?;
    let mut out = String::new();
    for ch in s.chars() {
        if ch.is_ascii_digit() {
            out.push(ch);
        } else {
            break;
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
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

pub fn parse_configs(fid: &str, gid: Option<&str>, html: &str) -> Vec<ConfigBase> {
    let doc = Html::parse_document(html);
    let card = Selector::parse(".card.cartitem").unwrap();
    let h4 = Selector::parse("h4").unwrap();
    let specs_block = Selector::parse(".card-text").unwrap();
    let a_price = Selector::parse("a.cart-num").unwrap();
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
        let price = Money {
            amount,
            currency: "CNY".to_string(),
            period: "month".to_string(),
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
            if href.contains("configureproduct&pid=") {
                source_pid = extract_query_number(href, "pid");
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
    fn parses_configs_with_inventory() {
        let html = include_str!("../tests/fixtures/cart-fid-7.html");
        let configs = parse_configs("7", Some("40"), html);
        let cfg = configs.iter().find(|c| c.name.contains("Basic")).unwrap();
        assert!(cfg.monitor_supported);
        assert!(cfg.inventory.quantity >= 0);
    }

    #[test]
    fn cloud_server_configs_disable_monitoring() {
        let html = include_str!("../tests/fixtures/cart-fid-2-gid-56.html");
        let configs = parse_configs("2", Some("56"), html);
        assert!(!configs.is_empty());
        assert!(!configs[0].monitor_supported);
        assert_eq!(configs[0].inventory.quantity, 1);
    }
}
