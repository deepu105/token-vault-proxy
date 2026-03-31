use anyhow::Result;
use colored::Colorize;
use std::io::{self, BufRead, Write};

use crate::store::credential_store::CredentialStore;
use crate::store::types::StoredConfig;
use crate::utils::output::output;

fn prompt(label: &str, default: Option<&str>) -> Result<String> {
    let suffix = match default {
        Some(d) => format!(" [{}]", d),
        None => String::new(),
    };
    eprint!("{}{}: ", label, suffix);
    io::stderr().flush()?;

    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    let trimmed = line.trim().to_string();

    if trimmed.is_empty() {
        if let Some(d) = default {
            return Ok(d.to_string());
        }
    }

    Ok(trimmed)
}

pub async fn run(json_mode: bool) -> Result<()> {
    let store = CredentialStore::from_env()?;
    let existing = store.get_config()?;

    eprintln!("{}", "Auth0 Token Vault Proxy — Setup Wizard\n".bold());

    let domain = prompt("Auth0 domain", existing.as_ref().map(|c| c.domain.as_str()))?;
    if domain.is_empty() {
        anyhow::bail!("Auth0 domain is required.");
    }

    let client_id = prompt("Client ID", existing.as_ref().map(|c| c.client_id.as_str()))?;
    if client_id.is_empty() {
        anyhow::bail!("Client ID is required.");
    }

    let secret_default = existing.as_ref().map(|c| c.client_secret.as_str());
    let secret_display = secret_default.map(|_| "****");
    let client_secret_input = prompt("Client Secret", secret_display)?;
    let client_secret = if client_secret_input.is_empty() || client_secret_input == "****" {
        match secret_default {
            Some(s) => s.to_string(),
            None => anyhow::bail!("Client Secret is required."),
        }
    } else {
        client_secret_input
    };
    if client_secret.is_empty() {
        anyhow::bail!("Client Secret is required.");
    }

    let audience = prompt(
        "API Audience (optional, press Enter to skip)",
        existing.as_ref().and_then(|c| c.audience.as_deref()),
    )?;

    let config = StoredConfig {
        domain: domain.clone(),
        client_id: client_id.clone(),
        client_secret,
        audience: if audience.is_empty() {
            None
        } else {
            Some(audience)
        },
    };

    store.save_config(&config)?;

    output(
        serde_json::json!({
            "status": "configured",
            "domain": domain,
            "clientId": client_id,
        }),
        &format!(
            "{} Configuration saved! Run `tv-proxy login` to authenticate.",
            "✓".green()
        ),
        json_mode,
    );

    Ok(())
}
