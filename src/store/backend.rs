use anyhow::Result;

use super::types::{Auth0Tokens, ConnectionToken, ServiceSettings, StoredConfig};

/// Storage backend contract for credential persistence.
///
/// Both `KeyringBackend` and `FileBackend` implement this trait.
/// Expiry logic lives in the `CredentialStore` facade, not here.
pub trait CredentialBackend: Send + Sync {
    /// Retrieve stored Auth0 configuration.
    fn get_config(&self) -> Result<Option<StoredConfig>>;

    /// Persist Auth0 configuration.
    fn save_config(&self, config: &StoredConfig) -> Result<()>;

    /// Retrieve stored Auth0 tokens.
    fn get_auth0_tokens(&self) -> Result<Option<Auth0Tokens>>;

    /// Persist Auth0 tokens.
    fn save_auth0_tokens(&self, tokens: &Auth0Tokens) -> Result<()>;

    /// Retrieve a connection-specific token by provider/connection name.
    fn get_connection_token(&self, connection: &str) -> Result<Option<ConnectionToken>>;

    /// Persist a connection-specific token.
    fn save_connection_token(&self, connection: &str, token: &ConnectionToken) -> Result<()>;

    /// List all stored connection names.
    fn list_connections(&self) -> Result<Vec<String>>;

    /// Remove a stored connection token. Returns `Ok(())` even if absent.
    fn remove_connection(&self, connection: &str) -> Result<()>;

    /// Retrieve per-service settings (e.g., allowed domains).
    fn get_service_settings(&self, service: &str) -> Result<Option<ServiceSettings>>;

    /// Persist per-service settings.
    fn save_service_settings(&self, service: &str, settings: &ServiceSettings) -> Result<()>;

    /// Clear tokens and connections but preserve config and service settings.
    fn clear(&self) -> Result<()>;
}
