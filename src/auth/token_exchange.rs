use anyhow::{Context, Result};
use tracing::debug;

use crate::utils::config::Auth0Config;
use crate::utils::error::AppError;
use crate::utils::http::http_client;

const GRANT_TYPE: &str =
    "urn:auth0:params:oauth:grant-type:token-exchange:federated-connection-access-token";
const SUBJECT_TOKEN_TYPE: &str = "urn:ietf:params:oauth:token-type:refresh_token";
const REQUESTED_TOKEN_TYPE: &str =
    "http://auth0.com/oauth/token-type/federated-connection-access-token";

#[derive(serde::Deserialize)]
struct ExchangeResponse {
    access_token: String,
    expires_in: Option<i64>,
    scope: Option<String>,
}

#[derive(serde::Deserialize)]
struct ErrorResponse {
    error: Option<String>,
    error_description: Option<String>,
}

pub struct ExchangeResult {
    pub access_token: String,
    pub expires_in: i64,
    pub scopes: Vec<String>,
}

/// Exchange a refresh token for a federated connection access token.
/// Maps Auth0 error codes to appropriate AppError variants.
pub async fn exchange_for_connection_token(
    config: &Auth0Config,
    refresh_token: &str,
    connection: &str,
) -> Result<ExchangeResult> {
    debug!("exchanging token for connection {}", connection);

    let base = crate::utils::config::auth0_base_url(&config.domain);
    let token_endpoint = format!("{}/oauth/token", base);
    let http = http_client()?;

    let form = [
        ("grant_type", GRANT_TYPE),
        ("client_id", config.client_id.as_str()),
        ("client_secret", config.client_secret.as_str()),
        ("subject_token_type", SUBJECT_TOKEN_TYPE),
        ("subject_token", refresh_token),
        ("connection", connection),
        ("requested_token_type", REQUESTED_TOKEN_TYPE),
    ];

    let response = http
        .post(&token_endpoint)
        .form(&form)
        .send()
        .await
        .map_err(|e| AppError::NetworkError {
            message: format!("Token exchange request failed: {}", e),
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();

        let err_resp: ErrorResponse = serde_json::from_str(&body).unwrap_or(ErrorResponse {
            error: None,
            error_description: None,
        });

        let err_code = err_resp.error.as_deref().unwrap_or("unknown");
        let fallback = format!("HTTP {}", status);
        let err_desc = err_resp
            .error_description
            .as_deref()
            .unwrap_or(&fallback);

        debug!("token exchange failed: {} - {}", err_code, err_desc);

        return Err(match err_code {
            "unauthorized_client" | "access_denied" => AppError::AuthzRequired {
                message: format!(
                    "Connection {} not authorized. Run `tv-proxy connect <service>` first.",
                    connection
                ),
            }
            .into(),
            "invalid_grant" | "expired_token" => AppError::AuthRequired {
                message: "Session expired. Run `tv-proxy login` to re-authenticate.".to_string(),
            }
            .into(),
            "federated_connection_refresh_token_flow_failed" => AppError::AuthzRequired {
                message: format!(
                    "Connection {} token refresh failed. Run `tv-proxy connect <service>` to re-authorize.",
                    connection
                ),
            }
            .into(),
            _ => AppError::ServiceError {
                message: format!("Token exchange failed: {}", err_desc),
            }
            .into(),
        });
    }

    let data: ExchangeResponse = response
        .json()
        .await
        .context("Failed to parse exchange response")?;

    let scopes = data
        .scope
        .as_deref()
        .map(|s| s.split_whitespace().map(|s| s.to_string()).collect())
        .unwrap_or_default();

    Ok(ExchangeResult {
        access_token: data.access_token,
        expires_in: data.expires_in.unwrap_or(3600),
        scopes,
    })
}
