pub mod callback_server;
pub mod connected_accounts;
pub mod oidc_config;
pub mod pkce_flow;
pub mod token_exchange;
pub mod token_refresh;

use anyhow::{Context, Result};

/// Open a URL in the user's browser.
///
/// When `browser` is provided, spawns it directly as a command with the URL as
/// an argument. This avoids `open::with()` which on macOS wraps the call with
/// `open -a <program>` and only works with .app bundles, not scripts.
pub fn open_url(url: &str, browser: Option<&str>) -> Result<()> {
    if let Some(browser) = browser {
        std::process::Command::new(browser)
            .arg(url)
            .spawn()
            .with_context(|| format!("Failed to open browser: {browser}"))?;
    } else {
        open::that(url).context("Failed to open default browser")?;
    }
    Ok(())
}
