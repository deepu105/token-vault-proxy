use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Auth0 access/refresh/id tokens with absolute expiry timestamp.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

/// A connection-specific access token with scopes and absolute expiry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionToken {
    pub access_token: String,
    /// Absolute timestamp (ms since epoch) when this token expires.
    pub expires_at: i64,
    pub scopes: Vec<String>,
}

/// Stored Auth0 configuration (domain, client credentials, optional audience).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StoredConfig {
    pub domain: String,
    pub client_id: String,
    pub client_secret: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audience: Option<String>,
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
