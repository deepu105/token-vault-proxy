use anyhow::Result;
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

    eprintln!("Auth0 Token Vault Proxy — Setup Wizard\n");

    let domain = prompt(
        "Auth0 domain",
        existing.as_ref().map(|c| c.domain.as_str()),
    )?;
    if domain.is_empty() {
        anyhow::bail!("Auth0 domain is required.");
    }

    let client_id = prompt(
        "Client ID",
        existing.as_ref().map(|c| c.client_id.as_str()),
    )?;
    if client_id.is_empty() {
        anyhow::bail!("Client ID is required.");
    }

    let client_secret = prompt(
        "Client Secret",
        existing.as_ref().map(|c| c.client_secret.as_str()),
    )?;
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
        "Configuration saved! Run `tv-proxy login` to authenticate.",
        json_mode,
    );

    Ok(())
}
