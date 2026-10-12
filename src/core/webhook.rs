use reqwest::Client;
use serde::Serialize;

const VEEVI_API_URL: &str = "https://pay.getveevi.com/api/v1/sentinel/webhook";

#[derive(Serialize)]
pub struct WebhookPayload<'a> {
    pub database_id: &'a str,
    pub update_status: &'a str,
    pub error_message: Option<&'a str>,
    pub storage_size: Option<u64>,
    pub time_taken: Option<&'a str>,
}

pub async fn notify_webhook(license_key: &str, payload: &WebhookPayload<'_>) {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_else(|_| Client::new());

    let res = client.post(VEEVI_API_URL)
        .bearer_auth(license_key)
        .json(&payload)
        .send()
        .await;

    match res {
        Ok(response) if response.status().is_success() => {
            tracing::debug!("Veevi webhook triggered successfully.");
        }
        Ok(response) => {
            tracing::warn!("Veevi API rejected webhook request: {}. Check your license key.", response.status());
        }
        Err(e) => {
            tracing::warn!("Failed to reach Veevi API for webhook: {}. Skipping notification.", e);
        }
    }
}
