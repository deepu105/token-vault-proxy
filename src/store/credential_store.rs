use anyhow::Result;
use tracing::debug;

use super::backend::CredentialBackend;
use super::file_backend::FileBackend;
use super::keyring_backend::KeyringBackend;
use super::types::{Auth0Tokens, ConnectionToken, ServiceSettings, StoredConfig};

/// Proactive expiry buffer — treat tokens as expired 2 minutes early (ms).
pub const EXPIRY_BUFFER_MS: i64 = 2 * 60 * 1000;

/// High-level credential store facade.
///
/// Wraps a [`CredentialBackend`] with expiry checking and scope validation.
/// All token reads apply a 2-minute expiry buffer to avoid using tokens that
/// are about to expire mid-request.
pub struct CredentialStore {
    backend: Box<dyn CredentialBackend>,
}

impl CredentialStore {
    /// Create with an explicit backend (useful for testing).
    pub fn with_backend(backend: Box<dyn CredentialBackend>) -> Self {
        Self { backend }
    }

    /// Create from the `TV_PROXY_STORAGE` environment variable.
    ///
    /// - `"keyring"` (default) — uses the OS keychain via [`KeyringBackend`].
    /// - `"file"` — uses a JSON file via [`FileBackend`].
    pub fn from_env() -> Result<Self> {
        let storage =
            std::env::var("TV_PROXY_STORAGE").unwrap_or_else(|_| "keyring".to_string());
        debug!(storage = %storage, "selecting credential backend");
        match storage.as_str() {
            "file" => Ok(Self::with_backend(Box::new(FileBackend::new()))),
            "keyring" => Ok(Self::with_backend(Box::new(KeyringBackend::new()))),
            other => anyhow::bail!(
                "Invalid TV_PROXY_STORAGE value \"{}\". Must be \"keyring\" or \"file\".",
                other
            ),
        }
    }

    // ── Passthrough methods ──────────────────────────────────────────

    pub fn get_config(&self) -> Result<Option<StoredConfig>> {
        self.backend.get_config()
    }

    pub fn save_config(&self, config: &StoredConfig) -> Result<()> {
        self.backend.save_config(config)
    }

    pub fn save_auth0_tokens(&self, tokens: &Auth0Tokens) -> Result<()> {
        self.backend.save_auth0_tokens(tokens)
    }

    pub fn save_connection_token(
        &self,
        connection: &str,
        token: &ConnectionToken,
    ) -> Result<()> {
        self.backend.save_connection_token(connection, token)
    }

    pub fn list_connections(&self) -> Result<Vec<String>> {
        self.backend.list_connections()
    }

    pub fn remove_connection(&self, connection: &str) -> Result<()> {
        self.backend.remove_connection(connection)
    }

    pub fn get_service_settings(&self, service: &str) -> Result<Option<ServiceSettings>> {
        self.backend.get_service_settings(service)
    }

    pub fn save_service_settings(
        &self,
        service: &str,
        settings: &ServiceSettings,
    ) -> Result<()> {
        self.backend.save_service_settings(service, settings)
    }

    pub fn clear(&self) -> Result<()> {
        self.backend.clear()
    }

    // ── Expiry-aware methods ─────────────────────────────────────────

    /// Get Auth0 access token if present and not expired (with 2-min buffer).
    ///
    /// Returns `Ok(None)` if no tokens are stored or the access token has expired.
    pub fn get_auth0_token(&self) -> Result<Option<String>> {
        let tokens = match self.backend.get_auth0_tokens()? {
            Some(t) => t,
            None => return Ok(None),
        };
        if self.is_expired(tokens.expires_at) {
            debug!("Auth0 access token expired or within buffer window");
            return Ok(None);
        }
        Ok(Some(tokens.access_token))
    }

    /// Get raw Auth0 tokens regardless of expiry (for refresh operations).
    pub fn get_auth0_tokens(&self) -> Result<Option<Auth0Tokens>> {
        self.backend.get_auth0_tokens()
    }

    /// Get a connection access token if present, not expired, and scopes match.
    ///
    /// **Scope validation:** If `required_scopes` is non-empty and the cached
    /// token does not contain *all* required scopes, returns `Ok(None)` without
    /// invalidating the cache (institutional learning #2: scope-blind cache).
    ///
    /// **Expiry:** A 2-minute buffer is applied — tokens expiring within 2 minutes
    /// are treated as expired.
    pub fn get_connection_token(
        &self,
        connection: &str,
        required_scopes: &[&str],
    ) -> Result<Option<String>> {
        let entry = match self.backend.get_connection_token(connection)? {
            Some(e) => e,
            None => return Ok(None),
        };

        if self.is_expired(entry.expires_at) {
            debug!(connection, "connection token expired or within buffer window");
            return Ok(None);
        }

        if !required_scopes.is_empty() && !Self::has_all_scopes(&entry.scopes, required_scopes) {
            debug!(
                connection,
                required = ?required_scopes,
                cached = ?entry.scopes,
                "cached token missing required scopes"
            );
            return Ok(None);
        }

        Ok(Some(entry.access_token))
    }

    /// Get raw connection entry regardless of expiry (for scope checking, etc.).
    pub fn get_connection_entry(
        &self,
        connection: &str,
    ) -> Result<Option<ConnectionToken>> {
        self.backend.get_connection_token(connection)
    }

    // ── Private helpers ──────────────────────────────────────────────

    /// Check whether a token with the given `expires_at` timestamp is expired,
    /// applying the [`EXPIRY_BUFFER_MS`] proactive buffer.
    fn is_expired(&self, expires_at: i64) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        now >= expires_at - EXPIRY_BUFFER_MS
    }

    /// Returns `true` if `cached` contains every scope in `required`.
    fn has_all_scopes(cached: &[String], required: &[&str]) -> bool {
        required.iter().all(|r| cached.iter().any(|c| c == r))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::FileBackend;
    use tempfile::TempDir;

    /// Helper: current time in ms since epoch.
    fn now_ms() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    }

    /// Helper: build a CredentialStore backed by FileBackend in a temp dir.
    fn make_store(dir: &TempDir) -> CredentialStore {
        CredentialStore::with_backend(Box::new(FileBackend::with_dir(dir.path().to_path_buf())))
    }

    #[test]
    fn returns_none_for_absent_auth0_token() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);

        assert!(store.get_auth0_token().unwrap().is_none());
    }

    #[test]
    fn returns_auth0_token_when_not_expired() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);

        let tokens = Auth0Tokens {
            access_token: "at_valid".to_string(),
            refresh_token: Some("rt".to_string()),
            id_token: None,
            expires_at: now_ms() + 10 * 60 * 1000, // 10 minutes from now
        };
        store.save_auth0_tokens(&tokens).unwrap();

        let result = store.get_auth0_token().unwrap();
        assert_eq!(result, Some("at_valid".to_string()));
    }

    #[test]
    fn returns_none_when_auth0_token_expired() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);

        let tokens = Auth0Tokens {
            access_token: "at_expired".to_string(),
            refresh_token: Some("rt".to_string()),
            id_token: None,
            expires_at: now_ms() - 60_000, // 1 minute in the past
        };
        store.save_auth0_tokens(&tokens).unwrap();

        assert!(store.get_auth0_token().unwrap().is_none());
    }

    #[test]
    fn expiry_buffer_treats_nearly_expired_token_as_expired() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);

        // Token expires in 1 minute — within the 2-minute buffer
        let tokens = Auth0Tokens {
            access_token: "at_almost".to_string(),
            refresh_token: None,
            id_token: None,
            expires_at: now_ms() + 60_000, // 1 minute from now
        };
        store.save_auth0_tokens(&tokens).unwrap();

        assert!(
            store.get_auth0_token().unwrap().is_none(),
            "token expiring in 1 minute should be treated as expired (2-min buffer)"
        );
    }

    #[test]
    fn returns_none_for_absent_connection_token() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);

        assert!(store
            .get_connection_token("gmail", &[])
            .unwrap()
            .is_none());
    }

    #[test]
    fn returns_connection_token_when_valid() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);

        let token = ConnectionToken {
            access_token: "conn_valid".to_string(),
            expires_at: now_ms() + 10 * 60 * 1000,
            scopes: vec!["read".to_string(), "write".to_string()],
        };
        store.save_connection_token("gmail", &token).unwrap();

        let result = store.get_connection_token("gmail", &[]).unwrap();
        assert_eq!(result, Some("conn_valid".to_string()));
    }

    #[test]
    fn returns_none_when_connection_token_scopes_insufficient() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);

        let token = ConnectionToken {
            access_token: "conn_narrow".to_string(),
            expires_at: now_ms() + 10 * 60 * 1000,
            scopes: vec!["read".to_string()],
        };
        store.save_connection_token("gmail", &token).unwrap();

        // Require both "read" and "write" — cached only has "read"
        let result = store
            .get_connection_token("gmail", &["read", "write"])
            .unwrap();
        assert!(
            result.is_none(),
            "should return None when cached scopes don't cover all required scopes"
        );
    }

    #[test]
    fn returns_connection_token_when_scopes_match() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);

        let token = ConnectionToken {
            access_token: "conn_full".to_string(),
            expires_at: now_ms() + 10 * 60 * 1000,
            scopes: vec![
                "read".to_string(),
                "write".to_string(),
                "admin".to_string(),
            ],
        };
        store.save_connection_token("github", &token).unwrap();

        // Require subset of cached scopes — should succeed
        let result = store
            .get_connection_token("github", &["read", "write"])
            .unwrap();
        assert_eq!(result, Some("conn_full".to_string()));
    }

    #[test]
    fn returns_none_when_connection_token_expired() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);

        let token = ConnectionToken {
            access_token: "conn_expired".to_string(),
            expires_at: now_ms() - 60_000,
            scopes: vec!["read".to_string()],
        };
        store.save_connection_token("slack", &token).unwrap();

        assert!(store
            .get_connection_token("slack", &["read"])
            .unwrap()
            .is_none());
    }

    #[test]
    fn scope_check_does_not_invalidate_cache() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);

        let token = ConnectionToken {
            access_token: "conn_keep".to_string(),
            expires_at: now_ms() + 10 * 60 * 1000,
            scopes: vec!["read".to_string()],
        };
        store.save_connection_token("gmail", &token).unwrap();

        // Request wider scopes — should return None
        assert!(store
            .get_connection_token("gmail", &["read", "write"])
            .unwrap()
            .is_none());

        // Original token should still be in cache (not invalidated)
        let raw = store.get_connection_entry("gmail").unwrap();
        assert!(raw.is_some(), "cache entry must not be invalidated on scope mismatch");
        assert_eq!(raw.unwrap().access_token, "conn_keep");
    }

    #[test]
    fn get_auth0_tokens_returns_raw_regardless_of_expiry() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);

        let tokens = Auth0Tokens {
            access_token: "at_raw".to_string(),
            refresh_token: Some("rt_raw".to_string()),
            id_token: None,
            expires_at: now_ms() - 60_000, // expired
        };
        store.save_auth0_tokens(&tokens).unwrap();

        // get_auth0_tokens bypasses expiry check
        let raw = store.get_auth0_tokens().unwrap();
        assert!(raw.is_some());
        assert_eq!(raw.unwrap().access_token, "at_raw");

        // but get_auth0_token should return None
        assert!(store.get_auth0_token().unwrap().is_none());
    }

    #[test]
    fn empty_required_scopes_skips_scope_check() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);

        let token = ConnectionToken {
            access_token: "conn_any".to_string(),
            expires_at: now_ms() + 10 * 60 * 1000,
            scopes: vec![], // no scopes cached
        };
        store.save_connection_token("slack", &token).unwrap();

        // Empty required_scopes — should return the token regardless
        let result = store.get_connection_token("slack", &[]).unwrap();
        assert_eq!(result, Some("conn_any".to_string()));
    }

    // ── Round-trip save/retrieve ─────────────────────────────────────

    #[test]
    fn saves_and_retrieves_auth0_tokens() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);

        let tokens = Auth0Tokens {
            access_token: "at-123".to_string(),
            refresh_token: Some("rt-456".to_string()),
            id_token: None,
            expires_at: now_ms() + 60 * 60 * 1000,
        };
        store.save_auth0_tokens(&tokens).unwrap();

        let token = store.get_auth0_token().unwrap();
        assert_eq!(token, Some("at-123".to_string()));

        let full = store.get_auth0_tokens().unwrap();
        assert_eq!(full, Some(tokens));
    }

    #[test]
    fn saves_and_retrieves_connection_tokens() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);

        let token = ConnectionToken {
            access_token: "gmail-token-abc".to_string(),
            expires_at: now_ms() + 60 * 60 * 1000,
            scopes: vec!["https://www.googleapis.com/auth/gmail.modify".to_string()],
        };
        store.save_connection_token("google-oauth2", &token).unwrap();

        let result = store.get_connection_token("google-oauth2", &[]).unwrap();
        assert_eq!(result, Some("gmail-token-abc".to_string()));

        let entry = store.get_connection_entry("google-oauth2").unwrap();
        assert_eq!(entry, Some(token));
    }

    // ── List / remove / clear ───────────────────────────────────────

    #[test]
    fn lists_connected_services() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);

        let token1 = ConnectionToken {
            access_token: "t1".to_string(),
            expires_at: now_ms() + 60 * 60 * 1000,
            scopes: vec![],
        };
        let token2 = ConnectionToken {
            access_token: "t2".to_string(),
            expires_at: now_ms() + 60 * 60 * 1000,
            scopes: vec![],
        };
        store.save_connection_token("google-oauth2", &token1).unwrap();
        store.save_connection_token("slack", &token2).unwrap();

        let mut connections = store.list_connections().unwrap();
        connections.sort();
        assert_eq!(connections, vec!["google-oauth2", "slack"]);
    }

    #[test]
    fn removes_a_connection() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);

        let token = ConnectionToken {
            access_token: "t1".to_string(),
            expires_at: now_ms() + 60 * 60 * 1000,
            scopes: vec![],
        };
        store.save_connection_token("google-oauth2", &token).unwrap();

        store.remove_connection("google-oauth2").unwrap();
        assert!(store.get_connection_token("google-oauth2", &[]).unwrap().is_none());
        assert!(store.list_connections().unwrap().is_empty());
    }

    #[test]
    fn remove_nonexistent_connection_does_not_error() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);

        // Should not error
        store.remove_connection("unknown").unwrap();
    }

    #[test]
    fn clears_all_credentials() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);

        let tokens = Auth0Tokens {
            access_token: "at".to_string(),
            refresh_token: Some("rt".to_string()),
            id_token: None,
            expires_at: now_ms() + 60 * 60 * 1000,
        };
        store.save_auth0_tokens(&tokens).unwrap();

        let conn = ConnectionToken {
            access_token: "ct".to_string(),
            expires_at: now_ms() + 60 * 60 * 1000,
            scopes: vec![],
        };
        store.save_connection_token("google-oauth2", &conn).unwrap();

        store.clear().unwrap();

        assert!(store.get_auth0_token().unwrap().is_none());
        assert!(store.get_connection_token("google-oauth2", &[]).unwrap().is_none());
    }

    #[test]
    fn clear_is_safe_when_no_file_exists() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);

        // Should not error even though nothing has been saved
        store.clear().unwrap();
    }

    // ── File permissions ────────────────────────────────────────────

    #[cfg(unix)]
    #[test]
    fn creates_credential_file_with_0600_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);

        let tokens = Auth0Tokens {
            access_token: "at".to_string(),
            refresh_token: None,
            id_token: None,
            expires_at: now_ms() + 60 * 60 * 1000,
        };
        store.save_auth0_tokens(&tokens).unwrap();

        let file_path = dir.path().join("credentials.json");
        let metadata = std::fs::metadata(&file_path).unwrap();
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "credential file should have 0600 permissions");
    }

    // ── Corrupt file handling ───────────────────────────────────────

    #[test]
    fn throws_on_corrupt_credential_file() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);

        // Write corrupt data directly
        let file_path = dir.path().join("credentials.json");
        std::fs::write(&file_path, "not valid json").unwrap();

        // Must error rather than silently returning empty data
        assert!(store.get_auth0_token().is_err());
    }

    // ── Config storage ──────────────────────────────────────────────

    #[test]
    fn saves_and_retrieves_config() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);

        let config = StoredConfig {
            domain: "test.auth0.com".to_string(),
            client_id: "cid".to_string(),
            client_secret: "csec".to_string(),
            audience: Some("https://api.example.com".to_string()),
        };
        store.save_config(&config).unwrap();

        let loaded = store.get_config().unwrap();
        assert_eq!(loaded, Some(config));
    }

    #[test]
    fn returns_none_when_no_config_stored() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);

        assert!(store.get_config().unwrap().is_none());
    }

    #[test]
    fn config_survives_clear() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);

        let config = StoredConfig {
            domain: "test.auth0.com".to_string(),
            client_id: "cid".to_string(),
            client_secret: "csec".to_string(),
            audience: None,
        };
        store.save_config(&config).unwrap();

        let tokens = Auth0Tokens {
            access_token: "at".to_string(),
            refresh_token: None,
            id_token: None,
            expires_at: now_ms() + 60 * 60 * 1000,
        };
        store.save_auth0_tokens(&tokens).unwrap();

        store.clear().unwrap();

        // Tokens are gone
        assert!(store.get_auth0_token().unwrap().is_none());
        // Config is preserved
        let loaded = store.get_config().unwrap();
        assert_eq!(loaded.unwrap().domain, "test.auth0.com");
    }

    // ── Persistence across instances ────────────────────────────────

    #[test]
    fn data_persists_across_store_instances() {
        let dir = TempDir::new().unwrap();

        // Write with first store
        let store1 = make_store(&dir);
        let tokens = Auth0Tokens {
            access_token: "at-persist".to_string(),
            refresh_token: Some("rt-persist".to_string()),
            id_token: None,
            expires_at: now_ms() + 60 * 60 * 1000,
        };
        store1.save_auth0_tokens(&tokens).unwrap();
        let conn = ConnectionToken {
            access_token: "ct-persist".to_string(),
            expires_at: now_ms() + 60 * 60 * 1000,
            scopes: vec![],
        };
        store1.save_connection_token("google-oauth2", &conn).unwrap();

        // Read with a new store pointing at the same directory
        let store2 = make_store(&dir);
        assert_eq!(store2.get_auth0_token().unwrap(), Some("at-persist".to_string()));
        assert_eq!(
            store2.get_connection_token("google-oauth2", &[]).unwrap(),
            Some("ct-persist".to_string())
        );
    }

    // ── Service settings ────────────────────────────────────────────

    #[test]
    fn saves_and_retrieves_service_settings() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);

        let settings = ServiceSettings {
            allowed_domains: vec!["*.googleapis.com".to_string(), "api.example.com".to_string()],
        };
        store.save_service_settings("gmail", &settings).unwrap();

        let loaded = store.get_service_settings("gmail").unwrap();
        assert_eq!(loaded, Some(settings));
    }

    #[test]
    fn returns_none_for_absent_service_settings() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);

        assert!(store.get_service_settings("gmail").unwrap().is_none());
    }

    #[test]
    fn service_settings_survive_clear() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir);

        let settings = ServiceSettings {
            allowed_domains: vec!["*.googleapis.com".to_string()],
        };
        store.save_service_settings("gmail", &settings).unwrap();

        let tokens = Auth0Tokens {
            access_token: "at".to_string(),
            refresh_token: None,
            id_token: None,
            expires_at: now_ms() + 60 * 60 * 1000,
        };
        store.save_auth0_tokens(&tokens).unwrap();

        store.clear().unwrap();

        // Tokens are gone
        assert!(store.get_auth0_token().unwrap().is_none());
        // Service settings are preserved
        let loaded = store.get_service_settings("gmail").unwrap();
        assert_eq!(loaded, Some(settings));
    }
}
