use anyhow::{Context, Result};
use serde::Deserialize;
use tracing::debug;

use crate::utils::http::{check_response, http_client};

/// OIDC endpoints discovered from the Auth0 domain.
#[derive(Debug, Clone, Deserialize)]
pub struct OidcEndpoints {
    pub token_endpoint: String,
    pub authorization_endpoint: String,
    pub issuer: String,
}

/// Discover OIDC endpoints for the given Auth0 domain.
pub async fn discover(domain: &str) -> Result<OidcEndpoints> {
    let base = crate::utils::config::auth0_base_url(domain);
    let url = format!("{}/.well-known/openid-configuration", base);
    debug!("fetching OIDC configuration from {}", url);

    let http = http_client()?;

    let response = http
        .get(&url)
        .send()
        .await
        .context("OIDC discovery request failed")?;

    let response = check_response(response, "OIDC discovery failed").await?;

    response
        .json()
        .await
        .context("Failed to parse OIDC configuration")
}
