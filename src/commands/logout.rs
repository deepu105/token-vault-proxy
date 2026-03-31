use anyhow::Result;
use colored::Colorize;
use tracing::debug;

use crate::auth::callback_server::CallbackServer;
use crate::cli::LogoutArgs;
use crate::store::credential_store::CredentialStore;
use crate::utils::config::{merge_config, resolve_browser, resolve_callback_port};
use crate::utils::confirm::require_confirmation;
use crate::utils::output::output;

pub async fn run(args: LogoutArgs, browser: Option<String>, port: Option<u16>, json_mode: bool, confirmed: bool) -> Result<()> {
    let store = CredentialStore::from_env()?;

    let existing = store.get_auth0_tokens()?;
    if existing.is_none() {
        output(
            serde_json::json!({ "status": "not_logged_in" }),
            "Not logged in.",
            json_mode,
        );
        return Ok(());
    }

    require_confirmation("Logout will clear all credentials and connections", confirmed)?;

    // Browser logout if not --local
    if !args.local {
        let stored = store.get_config()?;
        let merged = merge_config(stored.as_ref());
        if let (Some(domain), Some(client_id)) = (&merged.domain, &merged.client_id) {
            let browser = resolve_browser(browser.as_deref());
            let port = resolve_callback_port(port);
            if let Err(e) = browser_logout(domain, client_id, browser, port).await {
                debug!("browser logout failed: {}", e);
            }
        }
    }

    store.clear()?;

    output(
        serde_json::json!({ "status": "logged_out" }),
        &format!("{} Logged out. All credentials and connections have been removed.", "✓".green()),
        json_mode,
    );

    Ok(())
}

async fn browser_logout(
    domain: &str,
    client_id: &str,
    browser: Option<String>,
    port: Option<u16>,
) -> Result<()> {
    let server = CallbackServer::bind(port).await?;
    let return_to = format!("http://127.0.0.1:{}/callback", server.port);

    let base = crate::utils::config::auth0_base_url(domain);
    let mut logout_url = url::Url::parse(&format!("{}/v2/logout", base))?;
    logout_url
        .query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("returnTo", &return_to);

    debug!("opening browser for logout: {}", logout_url);
    if let Some(ref b) = browser {
        open::with(logout_url.as_str(), b)?;
    } else {
        open::that(logout_url.as_str())?;
    }

    // Wait for the redirect back (this confirms the browser session ended)
    let _ = server.wait().await;
    Ok(())
}
