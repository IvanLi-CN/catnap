use crate::models::Money;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundNotification {
    pub telegram_text: String,
    pub web_push_title: String,
    pub web_push_body: String,
    pub web_push_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorEventKind {
    Restock,
    Price,
    Config,
}

impl MonitorEventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Restock => "restock",
            Self::Price => "price",
            Self::Config => "config",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Restock => "补货",
            Self::Price => "价格变动",
            Self::Config => "配置更新",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigLifecycleNotificationKind {
    Added,
    Removed,
}

impl ConfigLifecycleNotificationKind {
    fn label(self) -> &'static str {
        match self {
            Self::Added => "套餐新增",
            Self::Removed => "套餐已删除",
        }
    }

    fn summary_prefix(self) -> &'static str {
        match self {
            Self::Added => "库存",
            Self::Removed => "最近状态：库存",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopologyNotificationKind {
    RegionAdded,
    RegionRemoved,
    PartitionAdded,
    PartitionRemoved,
}

impl TopologyNotificationKind {
    fn label(self) -> &'static str {
        match self {
            Self::RegionAdded => "新地区",
            Self::RegionRemoved => "地区已删除",
            Self::PartitionAdded => "新可用区",
            Self::PartitionRemoved => "可用区已删除",
        }
    }

    fn target_label(self) -> &'static str {
        match self {
            Self::RegionAdded | Self::RegionRemoved => "地区",
            Self::PartitionAdded | Self::PartitionRemoved => "可用区",
        }
    }

    fn includes_catalog(self) -> bool {
        matches!(self, Self::RegionAdded | Self::PartitionAdded)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CatalogSummaryItem {
    pub name: String,
    pub price: Money,
}

pub struct MonitoringSnapshot<'a> {
    pub inventory_quantity: i64,
    pub price: &'a Money,
    pub digest: &'a str,
}

pub struct MonitoringChangeNotification {
    pub events: Vec<MonitorEventKind>,
    pub telegram_text: String,
}

pub fn build_monitoring_change_notification(
    name: &str,
    old: &MonitoringSnapshot<'_>,
    new: &MonitoringSnapshot<'_>,
    site_base_url: Option<&str>,
) -> Option<MonitoringChangeNotification> {
    let mut events = Vec::new();
    if old.inventory_quantity == 0 && new.inventory_quantity > 0 {
        events.push(MonitorEventKind::Restock);
    }
    if (old.price.amount - new.price.amount).abs() > f64::EPSILON {
        events.push(MonitorEventKind::Price);
    }
    if old.digest != new.digest {
        events.push(MonitorEventKind::Config);
    }

    if events.is_empty() {
        return None;
    }

    let title = events
        .iter()
        .map(|event| event.label())
        .collect::<Vec<_>>()
        .join(" + ");

    let mut lines = vec![format!("【{title}】{name}")];
    let mut summary = Vec::new();

    if events.contains(&MonitorEventKind::Restock) {
        summary.push(format!(
            "库存 {} → {}",
            old.inventory_quantity, new.inventory_quantity
        ));
    }

    if events.contains(&MonitorEventKind::Price) {
        summary.push(format!(
            "价格 {}",
            format_price_change(old.price, new.price)
        ));
    } else if events.contains(&MonitorEventKind::Restock) {
        summary.push(format_money(new.price));
    }

    if !summary.is_empty() {
        lines.push(summary.join("｜"));
    } else {
        lines.push(format!(
            "库存 {}｜{}",
            new.inventory_quantity,
            format_money(new.price)
        ));
    }

    if !events.contains(&MonitorEventKind::Restock) && events.contains(&MonitorEventKind::Price) {
        if let Some(last) = lines.last_mut() {
            last.push_str(&format!("｜库存 {}", new.inventory_quantity));
        }
    }

    if events.len() == 1 && events[0] == MonitorEventKind::Config {
        lines[1] = format!(
            "库存 {}｜{}",
            new.inventory_quantity,
            format_money(new.price)
        );
    } else if events.contains(&MonitorEventKind::Config) {
        lines.push("配置内容已更新".to_string());
    }

    if let Some(url) = monitoring_url(site_base_url) {
        lines.push(format!("查看监控：{url}"));
    }

    Some(MonitoringChangeNotification {
        events,
        telegram_text: lines.join("\n"),
    })
}

pub fn build_config_lifecycle_notification(
    kind: ConfigLifecycleNotificationKind,
    name: &str,
    partition_label: Option<&str>,
    quantity: i64,
    price: &Money,
    site_base_url: Option<&str>,
) -> OutboundNotification {
    let summary = format!(
        "{} {quantity}｜{}",
        kind.summary_prefix(),
        format_money(price)
    );

    let normalized_partition_label = partition_label
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let mut telegram_lines = vec![format!("【{}】{name}", kind.label())];
    if let Some(partition_label) = normalized_partition_label {
        telegram_lines.push(format!("范围：{partition_label}"));
    }
    telegram_lines.push(summary.clone());
    if let Some(url) = products_url(site_base_url) {
        telegram_lines.push(format!("查看全部产品：{url}"));
    }

    let web_push_body = if let Some(partition_label) = normalized_partition_label {
        format!("{partition_label}｜{name}｜{summary}")
    } else {
        format!("{name}｜{summary}")
    };

    OutboundNotification {
        telegram_text: telegram_lines.join("\n"),
        web_push_title: format!("Catnap · {}", kind.label()),
        web_push_body,
        web_push_url: "/products".to_string(),
    }
}

pub fn build_topology_notification(
    kind: TopologyNotificationKind,
    scope_label: &str,
    catalog_items: &[CatalogSummaryItem],
    total_catalog_count: usize,
    site_base_url: Option<&str>,
) -> OutboundNotification {
    let normalized_scope_label = scope_label.trim();
    let mut telegram_lines = vec![format!("【{}】{}", kind.label(), normalized_scope_label)];
    telegram_lines.push(format!(
        "{}：{}",
        kind.target_label(),
        normalized_scope_label
    ));

    if kind.includes_catalog() {
        if catalog_items.is_empty() {
            telegram_lines.push("当前未发现套餐。".to_string());
        } else {
            telegram_lines.push("当前套餐：".to_string());
            telegram_lines.extend(catalog_items.iter().enumerate().map(|(idx, item)| {
                format!("{}. {}｜{}", idx + 1, item.name, format_money(&item.price))
            }));
            if total_catalog_count > catalog_items.len() {
                telegram_lines.push(format!(
                    "其余 {} 个套餐未展开。",
                    total_catalog_count - catalog_items.len()
                ));
            }
        }
    }

    if let Some(url) = products_url(site_base_url) {
        telegram_lines.push(format!("查看全部产品：{url}"));
    }

    let web_push_body = if kind.includes_catalog() {
        if catalog_items.is_empty() {
            format!("{normalized_scope_label}｜当前未发现套餐")
        } else if total_catalog_count > catalog_items.len() {
            format!(
                "{normalized_scope_label}｜{} 个套餐，已展开前 {} 个",
                total_catalog_count,
                catalog_items.len()
            )
        } else {
            format!(
                "{normalized_scope_label}｜{} 个当前套餐",
                catalog_items.len()
            )
        }
    } else {
        normalized_scope_label.to_string()
    };

    OutboundNotification {
        telegram_text: telegram_lines.join("\n"),
        web_push_title: format!("Catnap · {}", kind.label()),
        web_push_body,
        web_push_url: "/products".to_string(),
    }
}

pub fn build_telegram_test_text(text_override: Option<&str>, now: OffsetDateTime) -> String {
    if let Some(text) = text_override.filter(|value| !value.trim().is_empty()) {
        return text.to_string();
    }

    let ts = now
        .replace_nanosecond(0)
        .unwrap_or(now)
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
        .replace('T', " ");

    format!(
        "【Telegram 测试】通知配置正常\n如果你看到这条消息，说明 Catnap 已可发送 Telegram 通知。\n时间：{ts}"
    )
}

pub fn build_web_push_test_notification(
    title_override: Option<&str>,
    body_override: Option<&str>,
    url_override: Option<&str>,
) -> OutboundNotification {
    OutboundNotification {
        telegram_text: String::new(),
        web_push_title: title_override
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("Catnap · 测试通知")
            .to_string(),
        web_push_body: body_override
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("Web Push 已连通，点击返回设置页。")
            .to_string(),
        web_push_url: url_override
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("/settings")
            .to_string(),
    }
}

pub fn format_money(money: &Money) -> String {
    let period_label = match money.period.as_str() {
        "month" => "月".to_string(),
        "year" => "年".to_string(),
        _ => money.period.clone(),
    };

    if money.currency == "CNY" {
        return format!("¥{:.2} / {period_label}", money.amount);
    }

    format!("{:.2} {}/{}", money.amount, money.currency, money.period)
}

fn format_price_change(old: &Money, new: &Money) -> String {
    if old.currency == new.currency && old.period == new.period {
        if old.currency == "CNY" {
            let period_label = match new.period.as_str() {
                "month" => "月",
                "year" => "年",
                other => other,
            };
            return format!("¥{:.2} → ¥{:.2} / {period_label}", old.amount, new.amount);
        }

        return format!(
            "{:.2} {}/{} → {:.2} {}/{}",
            old.amount, old.currency, old.period, new.amount, new.currency, new.period
        );
    }

    format!("{} → {}", format_money(old), format_money(new))
}

fn monitoring_url(site_base_url: Option<&str>) -> Option<String> {
    let base = site_base_url?.trim();
    if base.is_empty() {
        return None;
    }
    Some(format!("{}/monitoring", base.trim_end_matches('/')))
}

fn products_url(site_base_url: Option<&str>) -> Option<String> {
    let base = site_base_url?.trim();
    if base.is_empty() {
        return None;
    }
    Some(format!("{}/products", base.trim_end_matches('/')))
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn money(amount: f64, currency: &str, period: &str) -> Money {
        Money {
            amount,
            currency: currency.to_string(),
            period: period.to_string(),
        }
    }

    #[test]
    fn formats_cny_period_labels_like_ui() {
        assert_eq!(format_money(&money(4.99, "CNY", "year")), "¥4.99 / 年");
        assert_eq!(format_money(&money(4.99, "CNY", "month")), "¥4.99 / 月");
    }

    #[test]
    fn builds_config_only_notification_with_link() {
        let old_price = money(4.99, "CNY", "month");
        let new_price = money(4.99, "CNY", "year");
        let old = MonitoringSnapshot {
            inventory_quantity: 0,
            price: &old_price,
            digest: "old",
        };
        let new = MonitoringSnapshot {
            inventory_quantity: 0,
            price: &new_price,
            digest: "new",
        };

        let notification = build_monitoring_change_notification(
            "芬兰特惠年付 Mini",
            &old,
            &new,
            Some("https://catnap.example"),
        )
        .expect("notification should exist");

        assert_eq!(notification.events, vec![MonitorEventKind::Config]);
        assert_eq!(
            notification.telegram_text,
            "【配置更新】芬兰特惠年付 Mini\n库存 0｜¥4.99 / 年\n查看监控：https://catnap.example/monitoring"
        );
    }

    #[test]
    fn builds_restock_price_notification_without_link() {
        let old_price = money(4.99, "CNY", "year");
        let new_price = money(3.99, "CNY", "year");
        let old = MonitoringSnapshot {
            inventory_quantity: 0,
            price: &old_price,
            digest: "same",
        };
        let new = MonitoringSnapshot {
            inventory_quantity: 3,
            price: &new_price,
            digest: "same",
        };

        let notification =
            build_monitoring_change_notification("芬兰特惠年付 Mini", &old, &new, None)
                .expect("notification should exist");

        assert_eq!(
            notification.events,
            vec![MonitorEventKind::Restock, MonitorEventKind::Price]
        );
        assert_eq!(
            notification.telegram_text,
            "【补货 + 价格变动】芬兰特惠年付 Mini\n库存 0 → 3｜价格 ¥4.99 → ¥3.99 / 年"
        );
    }

    #[test]
    fn builds_config_added_notification_for_products_page() {
        let notification = build_config_lifecycle_notification(
            ConfigLifecycleNotificationKind::Added,
            "芬兰特惠年付 Mini",
            Some("德国 / 德国特惠"),
            5,
            &money(4.99, "CNY", "year"),
            Some("https://catnap.example/base/"),
        );

        assert_eq!(notification.web_push_title, "Catnap · 套餐新增");
        assert_eq!(
            notification.web_push_body,
            "德国 / 德国特惠｜芬兰特惠年付 Mini｜库存 5｜¥4.99 / 年"
        );
        assert_eq!(notification.web_push_url, "/products");
        assert_eq!(
            notification.telegram_text,
            "【套餐新增】芬兰特惠年付 Mini
范围：德国 / 德国特惠
库存 5｜¥4.99 / 年
查看全部产品：https://catnap.example/base/products"
        );
    }

    #[test]
    fn builds_config_removed_notification_with_latest_state() {
        let notification = build_config_lifecycle_notification(
            ConfigLifecycleNotificationKind::Removed,
            "德国特惠年付 Mini",
            Some("德国 / 德国特惠"),
            0,
            &money(9.99, "CNY", "year"),
            None,
        );

        assert_eq!(notification.web_push_title, "Catnap · 套餐已删除");
        assert_eq!(
            notification.web_push_body,
            "德国 / 德国特惠｜德国特惠年付 Mini｜最近状态：库存 0｜¥9.99 / 年"
        );
        assert_eq!(
            notification.telegram_text,
            "【套餐已删除】德国特惠年付 Mini
范围：德国 / 德国特惠
最近状态：库存 0｜¥9.99 / 年"
        );
    }

    #[test]
    fn builds_region_added_notification_with_catalog_summary() {
        let notification = build_topology_notification(
            TopologyNotificationKind::RegionAdded,
            "德国",
            &[
                CatalogSummaryItem {
                    name: "德国特惠年付 Mini".to_string(),
                    price: money(9.99, "CNY", "year"),
                },
                CatalogSummaryItem {
                    name: "德国特惠月付 Pro".to_string(),
                    price: money(19.99, "CNY", "month"),
                },
            ],
            3,
            Some("https://catnap.example/base"),
        );

        assert_eq!(notification.web_push_title, "Catnap · 新地区");
        assert_eq!(notification.web_push_body, "德国｜3 个套餐，已展开前 2 个");
        assert_eq!(notification.web_push_url, "/products");
        assert_eq!(
            notification.telegram_text,
            "【新地区】德国
地区：德国
当前套餐：
1. 德国特惠年付 Mini｜¥9.99 / 年
2. 德国特惠月付 Pro｜¥19.99 / 月
其余 1 个套餐未展开。
查看全部产品：https://catnap.example/base/products"
        );
    }

    #[test]
    fn builds_partition_removed_notification_without_catalog_summary() {
        let notification = build_topology_notification(
            TopologyNotificationKind::PartitionRemoved,
            "德国 / 德国特惠",
            &[],
            0,
            None,
        );

        assert_eq!(notification.web_push_title, "Catnap · 可用区已删除");
        assert_eq!(notification.web_push_body, "德国 / 德国特惠");
        assert_eq!(
            notification.telegram_text,
            "【可用区已删除】德国 / 德国特惠
可用区：德国 / 德国特惠"
        );
    }

    #[test]
    fn builds_default_telegram_test_text() {
        let text = build_telegram_test_text(None, datetime!(2026-03-06 15:00:00 UTC));
        assert_eq!(
            text,
            "【Telegram 测试】通知配置正常\n如果你看到这条消息，说明 Catnap 已可发送 Telegram 通知。\n时间：2026-03-06 15:00:00Z"
        );
    }

    #[test]
    fn builds_default_web_push_test_notification() {
        let notification = build_web_push_test_notification(None, None, None);
        assert_eq!(notification.web_push_title, "Catnap · 测试通知");
        assert_eq!(
            notification.web_push_body,
            "Web Push 已连通，点击返回设置页。"
        );
        assert_eq!(notification.web_push_url, "/settings");
    }
}
