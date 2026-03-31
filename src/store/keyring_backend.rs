use anyhow::{Context, Result};
use keyring::Entry;
use tracing::debug;

use super::backend::CredentialBackend;
use super::types::{Auth0Tokens, ConnectionToken, ServiceSettings, StoredConfig};

const SERVICE: &str = "tv-proxy";

// Account names for keyring entries
const ACCOUNT_CONFIG: &str = "AUTH0_CONFIG";
const ACCOUNT_TOKENS: &str = "AUTH0_TOKENS";
const ACCOUNT_CONNECTION_LIST: &str = "CONNECTION_LIST";

/// Prefix for per-connection keyring entries.
fn connection_account(name: &str) -> String {
    format!("CONNECTION:{name}")
}

/// Prefix for per-service settings keyring entries.
fn settings_account(service: &str) -> String {
    format!("SETTINGS:{service}")
}

/// OS keychain credential backend.
///
/// Each piece of credential data lives in its own keyring entry under the
/// `tv-proxy` service name, matching the Node.js reference implementation's
/// account-per-key layout. Connection names are tracked in a dedicated
/// `CONNECTION_LIST` entry (JSON array) since the keyring crate v3 does not
/// support enumerating entries.
pub struct KeyringBackend;

impl KeyringBackend {
    pub fn new() -> Self {
        Self
    }

    /// Read a keyring entry and deserialize from JSON.
    /// Returns `Ok(None)` when the entry does not exist.
    fn get_json<T: serde::de::DeserializeOwned>(&self, account: &str) -> Result<Option<T>> {
        let entry = Entry::new(SERVICE, account)
            .with_context(|| format!("failed to create keyring entry for {account}"))?;

        match entry.get_password() {
            Ok(raw) => {
                let value = serde_json::from_str(&raw)
                    .with_context(|| format!("failed to parse keyring value for {account}"))?;
                Ok(Some(value))
            }
            Err(keyring::Error::NoEntry) => {
                debug!("no keyring entry for {account}");
                Ok(None)
            }
            Err(err) => {
                Err(err).with_context(|| format!("failed to read keyring entry for {account}"))
            }
        }
    }

    /// Serialize to JSON and store in a keyring entry.
    fn set_json<T: serde::Serialize>(&self, account: &str, value: &T) -> Result<()> {
        let entry = Entry::new(SERVICE, account)
            .with_context(|| format!("failed to create keyring entry for {account}"))?;

        let json = serde_json::to_string(value)
            .with_context(|| format!("failed to serialize value for {account}"))?;

        entry
            .set_password(&json)
            .with_context(|| format!("failed to write keyring entry for {account}"))?;

        debug!("saved keyring entry for {account}");
        Ok(())
    }

    /// Delete a keyring entry. Returns `Ok(())` if the entry does not exist.
    fn delete_entry(&self, account: &str) -> Result<()> {
        let entry = Entry::new(SERVICE, account)
            .with_context(|| format!("failed to create keyring entry for {account}"))?;

        match entry.delete_credential() {
            Ok(()) => {
                debug!("deleted keyring entry for {account}");
                Ok(())
            }
            Err(keyring::Error::NoEntry) => {
                debug!("no keyring entry to delete for {account}");
                Ok(())
            }
            Err(err) => {
                Err(err).with_context(|| format!("failed to delete keyring entry for {account}"))
            }
        }
    }

    /// Load the connection name list from the metadata entry.
    fn load_connection_list(&self) -> Result<Vec<String>> {
        Ok(self
            .get_json::<Vec<String>>(ACCOUNT_CONNECTION_LIST)?
            .unwrap_or_default())
    }

    /// Persist the connection name list to the metadata entry.
    fn save_connection_list(&self, names: &[String]) -> Result<()> {
        self.set_json(ACCOUNT_CONNECTION_LIST, &names)
    }
}

impl CredentialBackend for KeyringBackend {
    fn get_config(&self) -> Result<Option<StoredConfig>> {
        self.get_json(ACCOUNT_CONFIG)
    }

    fn save_config(&self, config: &StoredConfig) -> Result<()> {
        self.set_json(ACCOUNT_CONFIG, config)
    }

    fn get_auth0_tokens(&self) -> Result<Option<Auth0Tokens>> {
        self.get_json(ACCOUNT_TOKENS)
    }

    fn save_auth0_tokens(&self, tokens: &Auth0Tokens) -> Result<()> {
        self.set_json(ACCOUNT_TOKENS, tokens)
    }

    fn get_connection_token(&self, connection: &str) -> Result<Option<ConnectionToken>> {
        self.get_json(&connection_account(connection))
    }

    fn save_connection_token(&self, connection: &str, token: &ConnectionToken) -> Result<()> {
        self.set_json(&connection_account(connection), token)?;

        // Update the connection list metadata
        let mut names = self.load_connection_list()?;
        if !names.iter().any(|n| n == connection) {
            names.push(connection.to_string());
            self.save_connection_list(&names)?;
        }

        Ok(())
    }

    fn list_connections(&self) -> Result<Vec<String>> {
        self.load_connection_list()
    }

    fn remove_connection(&self, connection: &str) -> Result<()> {
        self.delete_entry(&connection_account(connection))?;

        // Update the connection list metadata
        let mut names = self.load_connection_list()?;
        names.retain(|n| n != connection);
        self.save_connection_list(&names)?;

        Ok(())
    }

    fn get_service_settings(&self, service: &str) -> Result<Option<ServiceSettings>> {
        self.get_json(&settings_account(service))
    }

    fn save_service_settings(&self, service: &str, settings: &ServiceSettings) -> Result<()> {
        self.set_json(&settings_account(service), settings)
    }

    fn clear(&self) -> Result<()> {
        // Delete Auth0 tokens
        self.delete_entry(ACCOUNT_TOKENS)?;

        // Delete each connection entry
        let names = self.load_connection_list()?;
        for name in &names {
            self.delete_entry(&connection_account(name))?;
        }

        // Clear the connection list
        self.delete_entry(ACCOUNT_CONNECTION_LIST)?;

        // Preserve AUTH0_CONFIG and SETTINGS:* entries
        debug!("cleared tokens and connections from keyring");
        Ok(())
    }
}
