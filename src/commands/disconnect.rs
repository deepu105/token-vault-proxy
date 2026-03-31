use anyhow::Result;
use colored::Colorize;

use crate::auth::connected_accounts::{delete_connected_account, list_connected_accounts};
use crate::cli::DisconnectArgs;
use crate::registry::{resolve_any, Resolution};
use crate::store::credential_store::CredentialStore;
use crate::utils::config::require_config;
use crate::utils::confirm::require_confirmation;
use crate::utils::error::AppError;
use crate::utils::output::output;

pub async fn run(args: DisconnectArgs, json_mode: bool, confirmed: bool) -> Result<()> {
    let store = CredentialStore::from_env()?;

    // Resolve connection name
    let resolution = resolve_any(&args.provider);
    let connection = match &resolution {
        Resolution::ProviderMatch(provider) => provider.connection.to_string(),
        Resolution::ServiceMatch(provider, _) => provider.connection.to_string(),
        Resolution::Unknown(_) => {
            return Err(AppError::InvalidInput {
                message: format!("Unknown provider or service: {}", args.provider),
            }
            .into());
        }
    };

    let service_name = args.provider.to_lowercase();

    require_confirmation(
        &format!(
            "Disconnect {}{}",
            service_name,
            if args.remote { " (local + remote)" } else { "" }
        ),
        confirmed,
    )?;
    let mut remote_deleted = false;

    if args.remote {
        let stored = store.get_config()?;
        let config = require_config(stored.as_ref())?;

        let auth0_tokens = store.get_auth0_tokens()?;
        let refresh_token = auth0_tokens
            .as_ref()
            .and_then(|t| t.refresh_token.as_deref())
            .ok_or_else(|| AppError::AuthRequired {
                message: "Not logged in. Run `tv-proxy login` first.".to_string(),
            })?;

        match list_connected_accounts(&config, refresh_token).await {
            Ok(accounts) => {
                if let Some(account) = accounts.iter().find(|a| a.connection == connection) {
                    match delete_connected_account(&config, refresh_token, &account.id).await {
                        Ok(()) => {
                            remote_deleted = true;
                        }
                        Err(e) => {
                            let msg = e.to_string();
                            output(
                                serde_json::json!({ "status": "warning", "message": format!("Remote disconnect failed: {}", msg) }),
                                &format!(
                                    "{} Remote disconnect failed — {}",
                                    "Warning:".yellow(),
                                    msg
                                ),
                                json_mode,
                            );
                        }
                    }
                } else {
                    output(
                        serde_json::json!({ "status": "warning", "message": format!("No remote connection found for {}", service_name) }),
                        &format!(
                            "{} No remote connection found for {}.",
                            "Warning:".yellow(),
                            service_name
                        ),
                        json_mode,
                    );
                }
            }
            Err(e) => {
                let msg = e.to_string();
                output(
                    serde_json::json!({ "status": "warning", "message": format!("Remote disconnect failed: {}", msg) }),
                    &format!("{} Remote disconnect failed — {}", "Warning:".yellow(), msg),
                    json_mode,
                );
            }
        }
    }

    // Always remove local token
    let _ = store.remove_connection(&connection);

    if remote_deleted {
        output(
            serde_json::json!({ "status": "disconnected", "service": service_name, "remote": true }),
            &format!(
                "{} Disconnected {} (local + remote).",
                "✓".green(),
                service_name
            ),
            json_mode,
        );
    } else {
        output(
            serde_json::json!({ "status": "disconnected", "service": service_name, "remote": false }),
            &format!("{} Disconnected {} (local).", "✓".green(), service_name),
            json_mode,
        );
    }

    Ok(())
}
