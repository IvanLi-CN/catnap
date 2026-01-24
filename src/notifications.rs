use anyhow::anyhow;
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
