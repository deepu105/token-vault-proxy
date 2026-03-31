use anyhow::Result;
use tracing::debug;

use crate::auth::connected_accounts::{run_connected_account_flow, list_connected_accounts, ConnectFlowOptions};
use crate::auth::token_exchange::exchange_for_connection_token;
use crate::cli::ConnectArgs;
use crate::registry::{resolve_any, Resolution, get_all_provider_scopes, get_service_scopes};
use crate::store::credential_store::CredentialStore;
use crate::store::types::{ConnectionToken, ServiceSettings};
use crate::utils::config::{require_config, resolve_browser, resolve_callback_port};
use crate::utils::error::AppError;
use crate::utils::output::output;

pub async fn run(args: ConnectArgs, browser: Option<String>, port: Option<u16>, json_mode: bool) -> Result<()> {
    // Resolve provider/service from input (validate before auth check)
    let resolution = resolve_any(&args.provider);
    let (connection, service_name, scopes) = match &resolution {
        Resolution::ProviderMatch(provider) => {
            // If --service specified, resolve to specific service; otherwise use all provider scopes
            if let Some(ref svc) = args.service {
                let svc_scopes = get_service_scopes(provider.connection, svc);
                if svc_scopes.is_empty() {
                    return Err(AppError::InvalidInput {
                        message: format!("Unknown service '{}' under provider '{}'", svc, provider.connection),
                    }.into());
                }
                (provider.connection.to_string(), svc.clone(), svc_scopes.iter().map(|s| s.to_string()).collect::<Vec<_>>())
            } else {
                let all_scopes = get_all_provider_scopes(provider.connection);
                (provider.connection.to_string(), args.provider.to_lowercase(), all_scopes.iter().map(|s| s.to_string()).collect())
            }
        }
        Resolution::ServiceMatch(provider, service) => {
            let svc_scopes = service.scopes.iter().map(|s| s.to_string()).collect::<Vec<_>>();
            (provider.connection.to_string(), service.name.to_string(), svc_scopes)
        }
        Resolution::Unknown(_) => {
            return Err(AppError::InvalidInput {
                message: format!("Unknown provider or service: {}", args.provider),
            }.into());
        }
    };

    let store = CredentialStore::from_env()?;
    let stored = store.get_config()?;
    let config = require_config(stored.as_ref())?;

    // Must be logged in
    let auth0_tokens = store.get_auth0_tokens()?;
    let refresh_token = auth0_tokens
        .as_ref()
        .and_then(|t| t.refresh_token.as_deref())
        .ok_or_else(|| AppError::AuthRequired {
            message: "Not logged in. Run `tv-proxy login` first.".to_string(),
        })?
        .to_string();

    // Add any user-specified extra scopes
    let mut scopes = scopes;
    if let Some(ref extra) = args.scopes {
        for s in extra.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
            if !scopes.iter().any(|existing| existing == s) {
                scopes.push(s.to_string());
            }
        }
    }

    // Clear stale cached token
    let _ = store.remove_connection(&connection);

    // Merge existing remote scopes (institutional learning: scope overwrite fix)
    match list_connected_accounts(&config, &refresh_token).await {
        Ok(accounts) => {
            if let Some(existing) = accounts.iter().find(|a| a.connection == connection) {
                for s in &existing.scopes {
                    if !scopes.iter().any(|existing| existing == s) {
                        scopes.push(s.clone());
                    }
                }
                debug!("merged existing remote scopes for {}: {:?}", connection, scopes);
            }
        }
        Err(e) => {
            debug!("failed to fetch existing remote scopes: {}", e);
        }
    }

    eprintln!("Connecting {}... Opening browser for authorization.", service_name);

    let browser = resolve_browser(browser.as_deref());
    let port = resolve_callback_port(port);

    let result = run_connected_account_flow(ConnectFlowOptions {
        config: config.clone(),
        refresh_token: refresh_token.clone(),
        connection: connection.clone(),
        scopes: scopes.clone(),
        browser,
        port,
    }).await?;

    // Validate with a token exchange
    let mut warning: Option<String> = None;
    match exchange_for_connection_token(&config, &refresh_token, &connection).await {
        Ok(exchange_result) => {
            // Cache the token
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;
            let _ = store.save_connection_token(&connection, &ConnectionToken {
                access_token: exchange_result.access_token,
                expires_at: now_ms + exchange_result.expires_in * 1000,
                scopes: exchange_result.scopes,
            });
        }
        Err(e) => {
            let msg = e.to_string();
            eprintln!("Warning: Token exchange failed — {}", msg);
            warning = Some(msg);
        }
    }

    // Save allowed domains if provided
    if let Some(ref domains_str) = args.allowed_domains {
        let domains: Vec<String> = domains_str
            .split(',')
            .map(|d| d.trim().to_lowercase())
            .filter(|d| !d.is_empty())
            .collect();
        if !domains.is_empty() {
            store.save_service_settings(&service_name, &ServiceSettings {
                allowed_domains: domains,
            })?;
        }
    }

    let mut data = serde_json::json!({
        "status": if warning.is_some() { "connected_with_warning" } else { "connected" },
        "service": service_name,
        "connection": result.connection,
        "id": result.id,
        "scopes": result.scopes,
    });

    if let Some(ref w) = warning {
        data["warning"] = serde_json::json!(w);
    }

    if let Ok(Some(settings)) = store.get_service_settings(&service_name) {
        if !settings.allowed_domains.is_empty() {
            data["allowedDomains"] = serde_json::json!(settings.allowed_domains);
        }
    }

    let human = if let Some(ref w) = warning {
        format!("Connected {} with warning: {}", service_name, w)
    } else {
        format!("Successfully connected {}!", service_name)
    };

    output(data, &human, json_mode);
    Ok(())
}
