use anyhow::{Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use colored::Colorize;
use sha2::{Digest, Sha256};
use tracing::debug;
use url::Url;

use super::callback_server::CallbackServer;
use super::oidc_config;
use super::open_url;
use crate::store::types::Auth0Tokens;
use crate::utils::config::Auth0Config;
use crate::utils::http::{check_response, http_client};
use crate::utils::time::now_ms;

pub struct PkceFlowOptions {
    pub config: Auth0Config,
    pub connection: Option<String>,
    pub connection_scope: Option<String>,
    pub scope: Option<String>,
    pub browser: Option<String>,
    pub port: Option<u16>,
    pub extra_params: Vec<(String, String)>,
}

fn generate_code_verifier() -> String {
    use rand::RngExt;
    let mut rng = rand::rng();
    let bytes: Vec<u8> = (0..32).map(|_| rng.random::<u8>()).collect();
    URL_SAFE_NO_PAD.encode(&bytes)
}

fn generate_code_challenge(verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(hasher.finalize())
}

fn generate_state() -> String {
    use rand::RngExt;
    let mut rng = rand::rng();
    let bytes: Vec<u8> = (0..16).map(|_| rng.random::<u8>()).collect();
    URL_SAFE_NO_PAD.encode(&bytes)
}

#[derive(serde::Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    id_token: Option<String>,
    expires_in: Option<i64>,
}

/// Run the full PKCE authorization code flow:
/// 1. Discover OIDC endpoints
/// 2. Start local callback server
/// 3. Build authorization URL and open browser
/// 4. Wait for the authorization code callback
/// 5. Exchange the code for tokens
pub async fn run_pkce_flow(options: PkceFlowOptions) -> Result<Auth0Tokens> {
    let endpoints = oidc_config::discover(&options.config.domain).await?;

    let code_verifier = generate_code_verifier();
    let code_challenge = generate_code_challenge(&code_verifier);
    let state = generate_state();

    // Phase 1: bind callback server to learn the port
    let server = CallbackServer::bind(options.port).await?;
    let redirect_uri = format!("http://127.0.0.1:{}/callback", server.port);
    debug!("callback server listening on port {}", server.port);

    // Build the authorization URL
    let mut auth_url = Url::parse(&endpoints.authorization_endpoint)?;
    {
        let mut q = auth_url.query_pairs_mut();
        q.append_pair("response_type", "code");
        q.append_pair("client_id", &options.config.client_id);
        q.append_pair("redirect_uri", &redirect_uri);
        q.append_pair(
            "scope",
            options
                .scope
                .as_deref()
                .unwrap_or("openid profile email offline_access"),
        );
        q.append_pair("code_challenge", &code_challenge);
        q.append_pair("code_challenge_method", "S256");
        q.append_pair("state", &state);

        if let Some(ref audience) = options.config.audience {
            q.append_pair("audience", audience);
        }
        if let Some(ref connection) = options.connection {
            q.append_pair("connection", connection);
        }
        if let Some(ref cs) = options.connection_scope {
            q.append_pair("connection_scope", cs);
        }
        for (k, v) in &options.extra_params {
            q.append_pair(k, v);
        }
    }

    // Open browser
    debug!("opening browser to {}", auth_url);
    eprintln!("{}", "Opening browser for authorization...".dimmed());
    open_url(auth_url.as_str(), options.browser.as_deref())?;

    // Phase 2: wait for the callback
    let callback = server.wait().await?;

    if callback.state != state {
        anyhow::bail!("State mismatch — possible CSRF attack");
    }

    // Exchange authorization code for tokens
    let http = http_client()?;

    let response = http
        .post(&endpoints.token_endpoint)
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", options.config.client_id.as_str()),
            ("client_secret", options.config.client_secret.as_str()),
            ("code", &callback.code),
            ("redirect_uri", &redirect_uri),
            ("code_verifier", &code_verifier),
        ])
        .send()
        .await
        .context("Token exchange request failed")?;

    let response = check_response(response, "Token exchange failed").await?;

    let token_resp: TokenResponse = response
        .json()
        .await
        .context("Failed to parse token response")?;

    let expires_in = token_resp.expires_in.unwrap_or(86400);

    Ok(Auth0Tokens {
        access_token: token_resp.access_token,
        refresh_token: token_resp.refresh_token,
        id_token: token_resp.id_token,
        expires_at: now_ms() + expires_in * 1000,
    })
}
