use anyhow::Result;
use colored::Colorize;

use crate::auth::pkce_flow::{run_pkce_flow, PkceFlowOptions};
use crate::cli::LoginArgs;
use crate::store::credential_store::CredentialStore;
use crate::store::types::StoredConfig;
use crate::utils::config::{require_config, resolve_browser, resolve_callback_port};
use crate::utils::output::output;

pub async fn run(
    args: LoginArgs,
    browser: Option<String>,
    port: Option<u16>,
    json_mode: bool,
) -> Result<()> {
    let store = CredentialStore::from_env()?;

    // Resolve config from env + store
    let stored = store.get_config()?;
    let config = require_config(stored.as_ref())?;

    // Save resolved config so future runs don't need env vars
    store.save_config(&StoredConfig {
        domain: config.domain.clone(),
        client_id: config.client_id.clone(),
        client_secret: config.client_secret.clone(),
        audience: config.audience.clone(),
    })?;

    let existing = store.get_auth0_tokens()?;
    let reauthenticated = existing.is_some();

    let browser = resolve_browser(browser.as_deref());
    let port = resolve_callback_port(port);

    let tokens = run_pkce_flow(PkceFlowOptions {
        config,
        connection: args.connection,
        connection_scope: args.connection_scope,
        scope: args.scope,
        browser,
        port,
        extra_params: vec![],
    })
    .await?;

    store.save_auth0_tokens(&tokens)?;

    if reauthenticated {
        output(
            serde_json::json!({ "status": "logged_in", "reauthenticated": true }),
            &format!("{} Successfully re-authenticated!", "✓".green()),
            json_mode,
        );
    } else {
        output(
            serde_json::json!({ "status": "logged_in" }),
            &format!("{} Successfully logged in!", "✓".green()),
            json_mode,
        );
    }

    Ok(())
}
