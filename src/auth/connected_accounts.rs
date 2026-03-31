use anyhow::{Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::callback_server::CallbackServer;
use super::open_url;
use crate::utils::config::Auth0Config;
use crate::utils::http::{check_response, http_client};

const MY_ACCOUNT_SCOPES: &str =
    "create:me:connected_accounts read:me:connected_accounts delete:me:connected_accounts";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectedAccount {
    pub id: String,
    pub connection: String,
    pub scopes: Vec<String>,
}

#[derive(Deserialize)]
struct ConnectInitResponse {
    auth_session: String,
    connect_uri: String,
    connect_params: ConnectParams,
}

#[derive(Deserialize)]
struct ConnectParams {
    ticket: String,
}

#[derive(Deserialize)]
struct AccountsListResponse {
    accounts: Option<Vec<ConnectedAccount>>,
}

/// Get a My Account API token by exchanging refresh token with MRRT audience.
async fn get_my_account_token(config: &Auth0Config, refresh_token: &str) -> Result<String> {
    let base = crate::utils::config::auth0_base_url(&config.domain);
    let audience = format!("{}/me/", base);
    debug!("requesting my account token with audience {}", audience);

    let endpoints = super::oidc_config::discover(&config.domain).await?;
    let http = http_client()?;

    let response = http
        .post(&endpoints.token_endpoint)
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", config.client_id.as_str()),
            ("client_secret", config.client_secret.as_str()),
            ("refresh_token", refresh_token),
            ("audience", audience.as_str()),
            ("scope", MY_ACCOUNT_SCOPES),
        ])
        .send()
        .await
        .context("My Account token request failed")?;

    let response = check_response(response, "My Account token exchange failed").await?;

    #[derive(Deserialize)]
    struct Resp {
        access_token: String,
    }
    let data: Resp = response.json().await?;
    Ok(data.access_token)
}

async fn initiate_connect(
    config: &Auth0Config,
    my_account_token: &str,
    connection: &str,
    scopes: &[String],
    redirect_uri: &str,
    state: &str,
) -> Result<ConnectInitResponse> {
    debug!("initiating connected account for {}", connection);
    let http = http_client()?;
    let base = crate::utils::config::auth0_base_url(&config.domain);
    let url = format!("{}/me/v1/connected-accounts/connect", base);

    let mut body = serde_json::json!({
        "connection": connection,
        "redirect_uri": redirect_uri,
        "state": state,
    });
    // Only include scopes when non-empty — the API rejects an empty array
    if !scopes.is_empty() {
        body["scopes"] = serde_json::json!(scopes);
    }

    let response = http
        .post(&url)
        .header("Authorization", format!("Bearer {}", my_account_token))
        .json(&body)
        .send()
        .await
        .context("Initiate connect request failed")?;

    let response = check_response(response, "Initiate connect failed").await?;

    response
        .json()
        .await
        .context("Failed to parse initiate response")
}

async fn complete_connect(
    config: &Auth0Config,
    my_account_token: &str,
    auth_session: &str,
    connect_code: &str,
    redirect_uri: &str,
) -> Result<ConnectedAccount> {
    debug!("completing connected account link");
    let http = http_client()?;
    let base = crate::utils::config::auth0_base_url(&config.domain);
    let url = format!("{}/me/v1/connected-accounts/complete", base);

    let body = serde_json::json!({
        "auth_session": auth_session,
        "connect_code": connect_code,
        "redirect_uri": redirect_uri,
    });

    let response = http
        .post(&url)
        .header("Authorization", format!("Bearer {}", my_account_token))
        .json(&body)
        .send()
        .await
        .context("Complete connect request failed")?;

    let response = check_response(response, "Complete connect failed").await?;

    response
        .json()
        .await
        .context("Failed to parse complete response")
}

pub struct ConnectFlowOptions {
    pub config: Auth0Config,
    pub refresh_token: String,
    pub connection: String,
    pub scopes: Vec<String>,
    pub browser: Option<String>,
    pub port: Option<u16>,
}

/// Run the full Connected Accounts flow.
///
/// 1. Exchange for My Account API token
/// 2. Start callback server (so it's ready before the browser redirects)
/// 3. Call initiate_connect to get the connect_uri
/// 4. Open browser to connect_uri
/// 5. Wait for callback
/// 6. Complete the connection
pub async fn run_connected_account_flow(options: ConnectFlowOptions) -> Result<ConnectedAccount> {
    let my_account_token = get_my_account_token(&options.config, &options.refresh_token).await?;

    let server = CallbackServer::bind(options.port).await?;
    let redirect_uri = format!("http://127.0.0.1:{}/callback", server.port);
    debug!("callback server listening on port {}", server.port);
    eprintln!(
        "{}",
        format!(
            "Redirect server listening on http://127.0.0.1:{}",
            server.port
        )
        .dimmed()
    );

    // Generate state
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use rand::RngExt;
    let state_bytes: Vec<u8> = (0..16).map(|_| rand::rng().random::<u8>()).collect();
    let state = URL_SAFE_NO_PAD.encode(&state_bytes);

    // Initiate connect inline so API errors propagate immediately
    // (the callback server is already listening, so no race condition)
    let init = initiate_connect(
        &options.config,
        &my_account_token,
        &options.connection,
        &options.scopes,
        &redirect_uri,
        &state,
    )
    .await?;

    let mut connect_url =
        url::Url::parse(&init.connect_uri).context("Invalid connect_uri from server")?;
    connect_url
        .query_pairs_mut()
        .append_pair("ticket", &init.connect_params.ticket);

    debug!("opening browser to {}", connect_url);
    open_url(connect_url.as_str(), options.browser.as_deref())?;

    // Wait for callback
    let callback = server.wait().await?;

    if callback.state != state {
        anyhow::bail!("State mismatch — possible CSRF attack");
    }

    complete_connect(
        &options.config,
        &my_account_token,
        &init.auth_session,
        &callback.code,
        &redirect_uri,
    )
    .await
}

/// List connected accounts.
pub async fn list_connected_accounts(
    config: &Auth0Config,
    refresh_token: &str,
) -> Result<Vec<ConnectedAccount>> {
    let token = get_my_account_token(config, refresh_token).await?;
    let http = http_client()?;
    let base = crate::utils::config::auth0_base_url(&config.domain);
    let url = format!("{}/me/v1/connected-accounts/accounts", base);

    let response = http
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .context("List connected accounts failed")?;

    let response = check_response(response, "List connected accounts failed").await?;

    let data: AccountsListResponse = response.json().await?;
    Ok(data.accounts.unwrap_or_default())
}

/// Delete a connected account by ID.
pub async fn delete_connected_account(
    config: &Auth0Config,
    refresh_token: &str,
    account_id: &str,
) -> Result<()> {
    let token = get_my_account_token(config, refresh_token).await?;
    let http = http_client()?;
    let base = crate::utils::config::auth0_base_url(&config.domain);
    let url = format!("{}/me/v1/connected-accounts/accounts/{}", base, account_id);

    let response = http
        .delete(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .context("Delete connected account failed")?;

    check_response(response, "Delete connected account failed").await?;

    Ok(())
}
