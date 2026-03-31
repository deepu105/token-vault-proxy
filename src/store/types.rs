use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

/// Auth0 access/refresh/id tokens with absolute expiry timestamp.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Auth0Tokens {
    pub access_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
    /// Absolute timestamp (ms since epoch) when the access token expires.
    pub expires_at: i64,
}

impl fmt::Debug for Auth0Tokens {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Auth0Tokens")
            .field("access_token", &"[REDACTED]")
            .field("refresh_token", &self.refresh_token.as_ref().map(|_| "[REDACTED]"))
            .field("id_token", &self.id_token.as_ref().map(|_| "[REDACTED]"))
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// A connection-specific access token with scopes and absolute expiry.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionToken {
    pub access_token: String,
    /// Absolute timestamp (ms since epoch) when this token expires.
    pub expires_at: i64,
    pub scopes: Vec<String>,
}

impl fmt::Debug for ConnectionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConnectionToken")
            .field("access_token", &"[REDACTED]")
            .field("expires_at", &self.expires_at)
            .field("scopes", &self.scopes)
            .finish()
    }
}

/// Stored Auth0 configuration (domain, client credentials, optional audience).
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StoredConfig {
    pub domain: String,
    pub client_id: String,
    pub client_secret: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audience: Option<String>,
}

impl fmt::Debug for StoredConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StoredConfig")
            .field("domain", &self.domain)
            .field("client_id", &self.client_id)
            .field("client_secret", &"[REDACTED]")
            .field("audience", &self.audience)
            .finish()
    }
}

/// Per-service settings such as allowed domains for the `fetch` command.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceSettings {
    pub allowed_domains: Vec<String>,
}

/// Top-level credential data stored in the JSON file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CredentialData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<StoredConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth0: Option<Auth0Tokens>,
    #[serde(default)]
    pub connections: HashMap<String, ConnectionToken>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_settings: Option<HashMap<String, ServiceSettings>>,
}

impl Default for CredentialData {
    fn default() -> Self {
        Self {
            config: None,
            auth0: None,
            connections: HashMap::new(),
            service_settings: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth0_tokens_debug_redacts_secrets() {
        let tokens = Auth0Tokens {
            access_token: "secret_at".into(),
            refresh_token: Some("secret_rt".into()),
            id_token: Some("secret_id".into()),
            expires_at: 12345,
        };
        let debug = format!("{:?}", tokens);
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("secret_at"));
        assert!(!debug.contains("secret_rt"));
        assert!(!debug.contains("secret_id"));
        assert!(debug.contains("12345"));
    }

    #[test]
    fn connection_token_debug_redacts_secrets() {
        let token = ConnectionToken {
            access_token: "secret_cat".into(),
            expires_at: 99999,
            scopes: vec!["read".into()],
        };
        let debug = format!("{:?}", token);
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("secret_cat"));
        assert!(debug.contains("99999"));
        assert!(debug.contains("read"));
    }

    #[test]
    fn stored_config_debug_redacts_secret() {
        let config = StoredConfig {
            domain: "example.auth0.com".into(),
            client_id: "cid".into(),
            client_secret: "super_secret".into(),
            audience: None,
        };
        let debug = format!("{:?}", config);
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("super_secret"));
        assert!(debug.contains("example.auth0.com"));
        assert!(debug.contains("cid"));
    }
}
