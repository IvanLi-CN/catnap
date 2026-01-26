use anyhow::anyhow;
use std::net::IpAddr;
use std::time::Duration;

pub async fn send_telegram(
    api_base_url: &str,
    token: &str,
    chat_id: &str,
    text: &str,
) -> anyhow::Result<()> {
    let api_base_url = api_base_url.trim_end_matches('/');
    let url = format!("{api_base_url}/bot{token}/sendMessage");

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|_| anyhow!("telegram client init failed"))?;
    let res = client
        .post(url)
        .json(&serde_json::json!({
            "chat_id": chat_id,
            "text": text,
            "disable_web_page_preview": true,
        }))
        .send()
        .await
        .map_err(|_| anyhow!("telegram request failed"))?;

    if !res.status().is_success() {
        anyhow::bail!("telegram http {}", res.status());
    }

    Ok(())
}

pub async fn send_web_push(
    cfg: &crate::config::RuntimeConfig,
    subscription: &crate::models::WebPushSubscription,
    title: &str,
    body: &str,
    url: &str,
) -> anyhow::Result<()> {
    use web_push::{
        ContentEncoding, HyperWebPushClient, SubscriptionInfo, VapidSignatureBuilder,
        WebPushClient, WebPushMessageBuilder,
    };

    validate_web_push_endpoint(
        subscription.endpoint.as_str(),
        cfg.allow_insecure_local_web_push_endpoints,
    )
    .await?;

    let Some(vapid_private_key) = cfg.web_push_vapid_private_key.as_deref() else {
        anyhow::bail!("missing CATNAP_WEB_PUSH_VAPID_PRIVATE_KEY");
    };
    let Some(vapid_subject) = cfg.web_push_vapid_subject.as_deref() else {
        anyhow::bail!("missing CATNAP_WEB_PUSH_VAPID_SUBJECT");
    };

    let endpoint = subscription.endpoint.trim();
    let p256dh = subscription.keys.p256dh.trim();
    let auth = subscription.keys.auth.trim();
    if endpoint.is_empty() || p256dh.is_empty() || auth.is_empty() {
        anyhow::bail!("web push subscription incomplete");
    }

    let subscription_info = SubscriptionInfo::new(endpoint, p256dh, auth);

    let mut sig_builder = VapidSignatureBuilder::from_base64(vapid_private_key, &subscription_info)
        .map_err(|_| anyhow!("web push: invalid vapid private key"))?;
    sig_builder.add_claim("sub", vapid_subject);
    let signature = sig_builder
        .build()
        .map_err(|_| anyhow!("web push: vapid signature build failed"))?;

    let payload = serde_json::to_vec(&serde_json::json!({
        "title": title,
        "body": body,
        "url": url,
    }))?;

    let mut builder = WebPushMessageBuilder::new(&subscription_info);
    builder.set_payload(ContentEncoding::Aes128Gcm, &payload);
    builder.set_ttl(60);
    builder.set_vapid_signature(signature);

    let message = builder
        .build()
        .map_err(|_| anyhow!("web push: build failed"))?;
    let client = HyperWebPushClient::new();

    match tokio::time::timeout(Duration::from_secs(10), client.send(message)).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(err)) => anyhow::bail!("web push: {}", err.short_description()),
        Err(_) => anyhow::bail!("web push: timeout"),
    }
}

pub async fn validate_web_push_endpoint(
    endpoint: &str,
    allow_insecure_local: bool,
) -> anyhow::Result<()> {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        anyhow::bail!("subscription.endpoint is empty");
    }

    let uri: axum::http::Uri = endpoint.parse().map_err(|_| anyhow!("invalid url"))?;

    let scheme = uri.scheme_str().ok_or_else(|| anyhow!("missing scheme"))?;

    if allow_insecure_local {
        if scheme != "http" && scheme != "https" {
            anyhow::bail!("unsupported scheme");
        }
        return Ok(());
    }

    if scheme != "https" {
        anyhow::bail!("endpoint must be https");
    }

    let authority = uri.authority().ok_or_else(|| anyhow!("missing host"))?;
    let host = authority.host().trim().to_ascii_lowercase();

    if host == "localhost" || host.ends_with(".localhost") {
        anyhow::bail!("endpoint host not allowed");
    }

    if let Some(port) = authority.port_u16() {
        if port != 443 {
            anyhow::bail!("endpoint port not allowed");
        }
    }

    if let Ok(ip) = host.parse::<IpAddr>() {
        if !is_public_ip(ip) {
            anyhow::bail!("endpoint ip not public");
        }
        return Ok(());
    }

    let addrs = tokio::net::lookup_host((host.as_str(), 443))
        .await
        .map_err(|_| anyhow!("host resolve failed"))?;

    if addrs.map(|a| a.ip()).any(|ip| !is_public_ip(ip)) {
        anyhow::bail!("endpoint resolves to non-public ip");
    }

    Ok(())
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_multicast()
                || v4.is_unspecified()
                || v4.octets()[0] == 0)
        }
        IpAddr::V6(v6) => {
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || v6.is_unique_local()
                || v6.is_unicast_link_local())
        }
    }
}
