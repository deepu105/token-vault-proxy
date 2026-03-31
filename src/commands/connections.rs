use anyhow::Result;
use colored::Colorize;
use tracing::debug;

use crate::auth::connected_accounts::list_connected_accounts;
use crate::store::credential_store::{CredentialStore, EXPIRY_BUFFER_MS};
use crate::utils::config::require_config;
use crate::utils::output::output;

fn local_token_status(expires_at: Option<i64>) -> &'static str {
    match expires_at {
        None => "none",
        Some(exp) => {
            let now = crate::utils::time::now_ms();
            if now >= exp - EXPIRY_BUFFER_MS {
                "expired"
            } else {
                "valid"
            }
        }
    }
}

pub async fn run(json_mode: bool) -> Result<()> {
    let store = CredentialStore::from_env()?;

    // Try to fetch remote connected accounts if logged in
    let mut remote_accounts = None;
    let stored = store.get_config()?;
    if let Ok(config) = require_config(stored.as_ref()) {
        if let Ok(Some(tokens)) = store.get_auth0_tokens() {
            if let Some(ref rt) = tokens.refresh_token {
                match list_connected_accounts(&config, rt).await {
                    Ok(accounts) => {
                        remote_accounts = Some(accounts);
                    }
                    Err(e) => {
                        debug!("failed to fetch remote connections: {}", e);
                    }
                }
            }
        }
    }

    let mut entries = Vec::new();

    if let Some(ref accounts) = remote_accounts {
        for acct in accounts {
            let local_entry = store.get_connection_entry(&acct.connection)?;
            let token_status =
                local_token_status(local_entry.as_ref().map(|e| e.expires_at));

            entries.push(serde_json::json!({
                "connection": acct.connection,
                "service": acct.connection,
                "id": acct.id,
                "scopes": acct.scopes,
                "tokenStatus": token_status,
                "remote": true,
            }));
        }
    } else {
        let connections = store.list_connections()?;
        for conn in &connections {
            let entry = store.get_connection_entry(conn)?;
            let token_status = local_token_status(entry.as_ref().map(|e| e.expires_at));
            let scopes: Vec<String> = entry
                .as_ref()
                .map(|e| e.scopes.clone())
                .unwrap_or_default();

            entries.push(serde_json::json!({
                "connection": conn,
                "service": conn,
                "scopes": scopes,
                "tokenStatus": token_status,
                "remote": false,
            }));
        }
    }

    if entries.is_empty() {
        output(
            serde_json::json!({ "connections": [] }),
            &format!("{}", "No services connected. Use `tv-proxy connect <service>` to connect one.".dimmed()),
            json_mode,
        );
        return Ok(());
    }

    let heading = if remote_accounts.is_some() {
        "Connected services:".bold().to_string()
    } else {
        "Connected services (local only):".bold().to_string()
    };

    let human_lines: Vec<String> = entries
        .iter()
        .map(|e| {
            let svc = e["service"].as_str().unwrap_or("unknown");
            let conn = e["connection"].as_str().unwrap_or("");
            let status = e["tokenStatus"].as_str().unwrap_or("unknown");
            let status_colored = match status {
                "valid" => "valid".green().to_string(),
                "expired" => "expired".red().to_string(),
                _ => status.yellow().to_string(),
            };
            format!("  {} ({}) — local token: {}", svc.cyan(), conn, status_colored)
        })
        .collect();

    output(
        serde_json::json!({ "connections": entries }),
        &format!("{}\n{}", heading, human_lines.join("\n")),
        json_mode,
    );

    Ok(())
}
