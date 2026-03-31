use anyhow::{Context, Result};
use serde::Deserialize;
use tracing::debug;

/// OIDC endpoints discovered from the Auth0 domain.
#[derive(Debug, Clone)]
pub struct OidcEndpoints {
    pub token_endpoint: String,
    pub authorization_endpoint: String,
    pub issuer: String,
}

#[derive(Deserialize)]
struct OidcConfiguration {
    token_endpoint: String,
    authorization_endpoint: String,
    issuer: String,
}

/// Discover OIDC endpoints for the given Auth0 domain.
pub async fn discover(domain: &str) -> Result<OidcEndpoints> {
    let base = crate::utils::config::auth0_base_url(domain);
    let url = format!("{}/.well-known/openid-configuration", base);
    debug!("fetching OIDC configuration from {}", url);

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    let response = http
        .get(&url)
        .send()
        .await
        .context("OIDC discovery request failed")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("OIDC discovery failed (HTTP {}): {}", status, body);
    }

    let config: OidcConfiguration = response
        .json()
        .await
        .context("Failed to parse OIDC configuration")?;

    Ok(OidcEndpoints {
        token_endpoint: config.token_endpoint,
        authorization_endpoint: config.authorization_endpoint,
        issuer: config.issuer,
    })
}
