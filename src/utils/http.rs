use anyhow::{bail, Result};

const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Canonical HTTP client with standard timeout and connection pooling.
pub fn http_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(DEFAULT_TIMEOUT_SECS))
        .build()?)
}

/// Check that a response is successful, or bail with a contextual error.
///
/// Returns the response unchanged on success for chaining.
/// `token_exchange.rs` has custom error-code mapping and should NOT use this.
pub async fn check_response(
    response: reqwest::Response,
    context: &str,
) -> Result<reqwest::Response> {
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        bail!("{} (HTTP {}): {}", context, status, body);
    }
    Ok(response)
}
