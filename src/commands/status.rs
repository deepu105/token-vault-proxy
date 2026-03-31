use anyhow::Result;

use crate::store::credential_store::{CredentialStore, EXPIRY_BUFFER_MS};
use crate::utils::config::{merge_config, resolve_storage_backend};
use crate::utils::output::output;

pub async fn run(json_mode: bool) -> Result<()> {
    let store = CredentialStore::from_env()?;
    let stored_config = store.get_config()?;
    let merged = merge_config(stored_config.as_ref());
    let auth0_tokens = store.get_auth0_tokens()?;

    if auth0_tokens.is_none() {
        output(
            serde_json::json!({
                "loggedIn": false,
                "domain": merged.domain,
                "clientId": merged.client_id,
            }),
            &format!(
                "Not logged in. Run `tv-proxy login` to authenticate.{}{}",
                merged
                    .domain
                    .as_ref()
                    .map(|d| format!("\n  Domain:    {}", d))
                    .unwrap_or_default(),
                merged
                    .client_id
                    .as_ref()
                    .map(|c| format!("\n  Client ID: {}", c))
                    .unwrap_or_default(),
            ),
            json_mode,
        );
        return Ok(());
    }

    let tokens = auth0_tokens.unwrap();

    // Decode ID token for user info
    let (email, name, sub) = if let Some(ref id_token) = tokens.id_token {
        decode_id_token(id_token)
    } else {
        (None, None, None)
    };

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let expired = now_ms >= tokens.expires_at - EXPIRY_BUFFER_MS;

    let connections = store.list_connections()?;
    let storage = resolve_storage_backend().unwrap_or_else(|_| "unknown".to_string());

    let display_name = name
        .as_deref()
        .or(email.as_deref())
        .or(sub.as_deref())
        .unwrap_or("unknown");

    let data = serde_json::json!({
        "loggedIn": !expired,
        "domain": merged.domain,
        "clientId": merged.client_id,
        "storage": storage,
        "user": {
            "email": email,
            "name": name,
            "sub": sub,
        },
        "tokenStatus": if expired { "expired" } else { "valid" },
        "connections": connections,
    });

    let conn_line = if connections.is_empty() {
        "  No services connected".to_string()
    } else {
        format!("  Connected: {}", connections.join(", "))
    };

    let human = format!(
        "Auth0 Token Vault Status\n\n  Domain:    {}\n  Client ID: {}\n  User:      {}\n  Email:     {}\n  Storage:   {}\n  Session:   {}\n\n{}",
        merged.domain.as_deref().unwrap_or("n/a"),
        merged.client_id.as_deref().unwrap_or("n/a"),
        display_name,
        email.as_deref().unwrap_or("n/a"),
        storage,
        if expired { "expired" } else { "active" },
        conn_line,
    );

    output(data, &human, json_mode);
    Ok(())
}

/// Decode an ID token (JWT) to extract basic claims. Does NOT verify the signature.
fn decode_id_token(token: &str) -> (Option<String>, Option<String>, Option<String>) {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() < 2 {
        return (None, None, None);
    }

    let payload = match URL_SAFE_NO_PAD.decode(parts[1]) {
        Ok(bytes) => bytes,
        Err(_) => {
            // Try with padding added
            let padded = match parts[1].len() % 4 {
                2 => format!("{}==", parts[1]),
                3 => format!("{}=", parts[1]),
                _ => parts[1].to_string(),
            };
            match URL_SAFE_NO_PAD.decode(&padded) {
                Ok(bytes) => bytes,
                Err(_) => return (None, None, None),
            }
        }
    };

    #[derive(serde::Deserialize)]
    struct Claims {
        email: Option<String>,
        name: Option<String>,
        sub: Option<String>,
    }

    match serde_json::from_slice::<Claims>(&payload) {
        Ok(claims) => (claims.email, claims.name, claims.sub),
        Err(_) => (None, None, None),
    }
}
