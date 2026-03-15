use crate::models::NotificationRecordDeliveryView;
use anyhow::anyhow;
use serde::Deserialize;
use std::net::IpAddr;
use std::time::Duration;

const TELEGRAM_ERROR_TEXT_MAX_CHARS: usize = 280;
const TELEGRAM_ERROR_BODY_MAX_BYTES: usize = 8 * 1024;

#[derive(Debug, Deserialize)]
struct TelegramErrorBody {
    description: Option<String>,
    parameters: Option<TelegramErrorParameters>,
}

#[derive(Debug, Deserialize)]
struct TelegramErrorParameters {
    migrate_to_chat_id: Option<i64>,
    retry_after: Option<i64>,
}

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
        let status = res.status();
        let (body, truncated) =
            read_limited_response_text(res, TELEGRAM_ERROR_BODY_MAX_BYTES).await;
        anyhow::bail!("{}", build_telegram_error(status, &body, token, truncated));
    }

    Ok(())
}

pub async fn send_telegram_to_targets(
    api_base_url: &str,
    token: &str,
    targets: &[String],
    text: &str,
) -> Vec<NotificationRecordDeliveryView> {
    let mut deliveries = Vec::with_capacity(targets.len());
    for target in targets {
        let result = match send_telegram(api_base_url, token, target, text).await {
            Ok(()) => NotificationRecordDeliveryView {
                channel: "telegram".to_string(),
                target: target.clone(),
                status: "success".to_string(),
                error: None,
            },
            Err(err) => NotificationRecordDeliveryView {
                channel: "telegram".to_string(),
                target: target.clone(),
                status: "error".to_string(),
                error: Some(err.to_string()),
            },
        };
        deliveries.push(result);
    }
    deliveries
}

fn build_telegram_error(
    status: reqwest::StatusCode,
    body: &str,
    token: &str,
    body_truncated: bool,
) -> String {
    let mut message = format!("telegram http {status}");

    if let Ok(parsed) = serde_json::from_str::<TelegramErrorBody>(body) {
        if let Some(desc) = parsed
            .description
            .as_deref()
            .map(|v| redact_token(v, token))
            .map(|v| sanitize_error_text(&v))
            .filter(|v| !v.is_empty())
        {
            message.push_str(": ");
            message.push_str(&desc);
        }

        if let Some(params) = parsed.parameters {
            if let Some(chat_id) = params.migrate_to_chat_id {
                message.push_str(&format!(
                    "; migrate_to_chat_id={chat_id} (target may be migrated, try this chat id)"
                ));
            }
            if let Some(retry_after) = params.retry_after {
                message.push_str(&format!("; retry_after={retry_after}s"));
            }
        }

        if body_truncated {
            message.push_str("; upstream_body_truncated");
        }

        return message;
    }

    message.push_str(": upstream returned non-json error body");
    if body_truncated {
        message.push_str("; upstream_body_truncated");
    }
    message
}

async fn read_limited_response_text(
    mut res: reqwest::Response,
    max_bytes: usize,
) -> (String, bool) {
    let mut out = Vec::with_capacity(max_bytes.min(1024));
    let mut truncated = false;

    loop {
        match res.chunk().await {
            Ok(Some(chunk)) => {
                if out.len() >= max_bytes {
                    truncated = true;
                    break;
                }
                let remain = max_bytes - out.len();
                if chunk.len() > remain {
                    out.extend_from_slice(&chunk[..remain]);
                    truncated = true;
                    break;
                }
                out.extend_from_slice(&chunk);
            }
            Ok(None) => break,
            Err(_) => break,
        }
    }

    (String::from_utf8_lossy(&out).into_owned(), truncated)
}

fn sanitize_error_text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len().min(TELEGRAM_ERROR_TEXT_MAX_CHARS));
    let mut count = 0usize;
    let mut prev_space = true;
    let mut truncated = false;

    for ch in raw.chars() {
        let c = if ch.is_control() { ' ' } else { ch };
        if c.is_whitespace() {
            if prev_space {
                continue;
            }
            if count >= TELEGRAM_ERROR_TEXT_MAX_CHARS {
                truncated = true;
                break;
            }
            out.push(' ');
            count += 1;
            prev_space = true;
            continue;
        }

        if count >= TELEGRAM_ERROR_TEXT_MAX_CHARS {
            truncated = true;
            break;
        }

        out.push(c);
        count += 1;
        prev_space = false;
    }

    while out.ends_with(' ') {
        out.pop();
    }
    if truncated {
        out.push_str("...");
    }
    out
}

fn redact_token(text: &str, token: &str) -> String {
    let t = token.trim();
    if t.is_empty() {
        return text.to_string();
    }

    let mut redacted = text.replace(&format!("bot{t}"), "bot[REDACTED]");
    redacted = redact_with_optional_whitespace(&redacted, t);
    redacted = redact_url_encoded_token(&redacted, t);
    redacted
}

fn redact_url_encoded_token(text: &str, token: &str) -> String {
    let encoded_upper = percent_encode_url_component(token, b"0123456789ABCDEF");
    let encoded_lower = percent_encode_url_component(token, b"0123456789abcdef");
    if encoded_upper == token && encoded_lower == token {
        return text.to_string();
    }

    let mut redacted = text.replace(&format!("bot{encoded_upper}"), "bot[REDACTED]");
    if encoded_lower != encoded_upper {
        redacted = redacted.replace(&format!("bot{encoded_lower}"), "bot[REDACTED]");
    }

    redacted = redact_with_optional_whitespace(&redacted, &encoded_upper);
    if encoded_lower != encoded_upper {
        redacted = redact_with_optional_whitespace(&redacted, &encoded_lower);
    }
    redacted
}

fn percent_encode_url_component(value: &str, hex: &[u8; 16]) -> String {
    let mut out = String::with_capacity(value.len().saturating_mul(3));
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
            continue;
        }
        out.push('%');
        out.push(hex[(b >> 4) as usize] as char);
        out.push(hex[(b & 0x0f) as usize] as char);
    }

    out
}

fn redact_with_optional_whitespace(text: &str, token: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let token_chars: Vec<char> = token.chars().collect();
    if token_chars.is_empty() {
        return text.to_string();
    }

    let mut out = String::with_capacity(text.len());
    let mut i = 0usize;

    while i < chars.len() {
        let mut j = i;
        let mut k = 0usize;
        let mut consumed = false;

        while j < chars.len() && k < token_chars.len() {
            let c = chars[j];
            if c.is_whitespace() {
                if consumed {
                    j += 1;
                    continue;
                }
                break;
            }
            if c != token_chars[k] {
                break;
            }
            consumed = true;
            j += 1;
            k += 1;
        }

        if k == token_chars.len() && match_token_boundaries(&chars, i, j) {
            out.push_str("[REDACTED]");
            i = j;
            continue;
        }

        out.push(chars[i]);
        i += 1;
    }

    out
}

fn match_token_boundaries(chars: &[char], start: usize, end: usize) -> bool {
    let prev = if start > 0 {
        Some(chars[start - 1])
    } else {
        None
    };
    let next = chars.get(end).copied();

    let has_word_boundary =
        prev.is_none_or(is_redaction_boundary) && next.is_none_or(is_redaction_boundary);

    has_word_boundary || has_bot_prefix(chars, start)
}

fn is_redaction_boundary(ch: char) -> bool {
    !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-'
}

fn has_bot_prefix(chars: &[char], start: usize) -> bool {
    let mut idx = start;
    while idx > 0 && chars[idx - 1].is_whitespace() {
        idx -= 1;
    }
    if idx < 3 {
        return false;
    }
    chars[idx - 3].eq_ignore_ascii_case(&'b')
        && chars[idx - 2].eq_ignore_ascii_case(&'o')
        && chars[idx - 1].eq_ignore_ascii_case(&'t')
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
