use anyhow::{Context, Result};
use tracing::debug;

use crate::store::types::Auth0Tokens;
use crate::utils::config::Auth0Config;
use crate::utils::http::{check_response, http_client};
use crate::utils::time::now_ms;

use super::oidc_config;

#[derive(serde::Deserialize)]
struct RefreshResponse {
    access_token: String,
    refresh_token: Option<String>,
    id_token: Option<String>,
    expires_in: Option<i64>,
}

/// Refresh Auth0 tokens using a refresh token.
/// If the server returns a new refresh token (rotation enabled), use it.
/// Otherwise keep the existing refresh token.
pub async fn refresh_auth0_token(
    config: &Auth0Config,
    refresh_token: &str,
) -> Result<Auth0Tokens> {
    debug!("refreshing auth0 access token");

    let endpoints = oidc_config::discover(&config.domain).await?;
    let http = http_client()?;

    let response = http
        .post(&endpoints.token_endpoint)
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", config.client_id.as_str()),
            ("client_secret", config.client_secret.as_str()),
            ("refresh_token", refresh_token),
        ])
        .send()
        .await
        .context("Token refresh request failed")?;

    let response = check_response(response, "Token refresh failed").await?;

    let data: RefreshResponse = response
        .json()
        .await
        .context("Failed to parse refresh response")?;

    let expires_in = data.expires_in.unwrap_or(86400);

    debug!("auth0 access token refreshed successfully");

    Ok(Auth0Tokens {
        access_token: data.access_token,
        refresh_token: Some(
            data.refresh_token
                .unwrap_or_else(|| refresh_token.to_string()),
        ),
        id_token: data.id_token,
        expires_at: now_ms() + expires_in * 1000,
    })
}
