use anyhow::Result;
use colored::Colorize;

use crate::auth::pkce_flow::{run_pkce_flow, PkceFlowOptions};
use crate::cli::LoginArgs;
use crate::store::credential_store::CredentialStore;
use crate::utils::config::{resolve_browser, resolve_callback_port, Auth0Config};
use crate::utils::output::output;
use crate::utils::prompt::resolve_config_with_prompts;

pub async fn run(
    args: LoginArgs,
    browser: Option<String>,
    port: Option<u16>,
    json_mode: bool,
) -> Result<()> {
    let store = CredentialStore::from_env()?;

    // Resolve config: CLI flags > env vars > stored config > interactive prompts
    let stored = if args.reconfigure {
        None
    } else {
        store.get_config()?
    };

    let config = resolve_config_with_prompts(
        args.domain.as_deref(),
        args.client_id.as_deref(),
        args.client_secret.as_deref(),
        args.audience.as_deref(),
        stored.as_ref(),
    )?;

    // Persist resolved config so future runs don't need env vars or flags
    store.save_config(&config)?;

    let existing = store.get_auth0_tokens()?;
    let reauthenticated = existing.is_some();

    let browser = resolve_browser(browser.as_deref());
    let port = resolve_callback_port(port);

    let tokens = run_pkce_flow(PkceFlowOptions {
        config: Auth0Config {
            domain: config.domain,
            client_id: config.client_id,
            client_secret: config.client_secret,
            audience: config.audience,
        },
        connection: None,
        connection_scope: None,
        scope: None,
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
