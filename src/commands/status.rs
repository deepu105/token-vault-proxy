use anyhow::Result;
use colored::Colorize;

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
                "{}{}{}{}",
                "Not logged in.".yellow(),
                " Run `tv-proxy login` to authenticate.",
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
        format!("  {}", "No services connected".dimmed())
    } else {
        format!("  Connected: {}", connections.join(", ").cyan())
    };

    let session_display = if expired {
        "expired".red().to_string()
    } else {
        "active".green().to_string()
    };

    let human = format!(
        "{}\n\n  Domain:    {}\n  Client ID: {}\n  User:      {}\n  Email:     {}\n  Storage:   {}\n  Session:   {}\n\n{}",
        "Auth0 Token Vault Status".bold(),
        merged.domain.as_deref().unwrap_or("n/a"),
        merged.client_id.as_deref().unwrap_or("n/a"),
        display_name,
        email.as_deref().unwrap_or("n/a"),
        storage,
        session_display,
        conn_line,
    );

    output(data, &human, json_mode);
    Ok(())
}

/// Decode an ID token (JWT) to extract basic claims. Does NOT verify the signature.
fn decode_id_token(token: &str) -> (Option<String>, Option<String>, Option<String>) {
    #[derive(serde::Deserialize)]
    struct Claims {
        email: Option<String>,
        name: Option<String>,
        sub: Option<String>,
    }

    match jsonwebtoken::dangerous::insecure_decode::<Claims>(token) {
        Ok(data) => (data.claims.email, data.claims.name, data.claims.sub),
        Err(_) => (None, None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_id_token_extracts_claims() {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;

        let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD.encode(
            r#"{"sub":"auth0|user123","email":"test@example.com","name":"Test User","iss":"https://test.auth0.com/","aud":"test","exp":9999999999}"#,
        );
        let token = format!("{}.{}.fake-signature", header, payload);

        let (email, name, sub) = decode_id_token(&token);
        assert_eq!(email, Some("test@example.com".to_string()));
        assert_eq!(name, Some("Test User".to_string()));
        assert_eq!(sub, Some("auth0|user123".to_string()));
    }

    #[test]
    fn decode_id_token_returns_none_for_invalid() {
        let (email, name, sub) = decode_id_token("not-a-jwt");
        assert_eq!(email, None);
        assert_eq!(name, None);
        assert_eq!(sub, None);
    }

    #[test]
    fn decode_id_token_returns_none_for_empty() {
        let (email, name, sub) = decode_id_token("");
        assert_eq!(email, None);
        assert_eq!(name, None);
        assert_eq!(sub, None);
    }

    #[test]
    fn decode_id_token_handles_partial_claims() {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;

        let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD.encode(
            r#"{"sub":"auth0|user456","iss":"https://test.auth0.com/","aud":"test","exp":9999999999}"#,
        );
        let token = format!("{}.{}.fake", header, payload);

        let (email, name, sub) = decode_id_token(&token);
        assert_eq!(email, None);
        assert_eq!(name, None);
        assert_eq!(sub, Some("auth0|user456".to_string()));
    }
}
