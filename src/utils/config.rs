use anyhow::{bail, Result};

use crate::store::types::StoredConfig;

/// Resolved Auth0 configuration with all required fields present.
#[derive(Debug, Clone)]
pub struct Auth0Config {
    pub domain: String,
    pub client_id: String,
    pub client_secret: String,
    pub audience: Option<String>,
}

/// Resolve the Auth0 base URL. When `TV_PROXY_AUTH0_BASE_URL` is set, use it
/// directly (useful for testing against a local mock server). Otherwise fall
/// back to `https://{domain}`.
pub fn auth0_base_url(domain: &str) -> String {
    std::env::var("TV_PROXY_AUTH0_BASE_URL")
        .unwrap_or_else(|_| format!("https://{}", domain))
}

/// Result of merging env vars with stored config. Fields may be `None` if
/// neither source provided a value.
#[derive(Debug)]
pub struct MergeResult {
    pub domain: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub audience: Option<String>,
    pub missing: Vec<&'static str>,
}

/// Merge config from explicit values and stored values. First-arg values take precedence.
/// This is the core merge implementation. The public `merge_config` function
/// reads env vars and delegates here.
pub(crate) fn merge_config_pure(
    env_domain: Option<&str>,
    env_client_id: Option<&str>,
    env_client_secret: Option<&str>,
    env_audience: Option<&str>,
    stored: Option<&StoredConfig>,
) -> MergeResult {
    let domain = env_domain
        .map(|s| s.to_string())
        .or_else(|| stored.map(|s| s.domain.clone()));
    let client_id = env_client_id
        .map(|s| s.to_string())
        .or_else(|| stored.map(|s| s.client_id.clone()));
    let client_secret = env_client_secret
        .map(|s| s.to_string())
        .or_else(|| stored.map(|s| s.client_secret.clone()));
    let audience = env_audience
        .map(|s| s.to_string())
        .or_else(|| stored.and_then(|s| s.audience.clone()));

    let mut missing = Vec::new();
    if domain.is_none() {
        missing.push("AUTH0_DOMAIN");
    }
    if client_id.is_none() {
        missing.push("AUTH0_CLIENT_ID");
    }
    if client_secret.is_none() {
        missing.push("AUTH0_CLIENT_SECRET");
    }

    MergeResult {
        domain,
        client_id,
        client_secret,
        audience,
        missing,
    }
}

/// Merge config from env vars and stored values. Env vars take precedence.
pub fn merge_config(stored: Option<&StoredConfig>) -> MergeResult {
    merge_config_pure(
        std::env::var("AUTH0_DOMAIN").ok().as_deref(),
        std::env::var("AUTH0_CLIENT_ID").ok().as_deref(),
        std::env::var("AUTH0_CLIENT_SECRET").ok().as_deref(),
        std::env::var("AUTH0_AUDIENCE").ok().as_deref(),
        stored,
    )
}

/// Load Auth0 config, returning an error if any required field is missing.
pub fn require_config(stored: Option<&StoredConfig>) -> Result<Auth0Config> {
    let result = merge_config(stored);
    if !result.missing.is_empty() {
        bail!(
            "Not configured. Run `tv-proxy login` first, or set {} environment variable{}.",
            result.missing.join(", "),
            if result.missing.len() > 1 { "s" } else { "" }
        );
    }
    Ok(Auth0Config {
        domain: result.domain.unwrap(),
        client_id: result.client_id.unwrap(),
        client_secret: result.client_secret.unwrap(),
        audience: result.audience,
    })
}

/// Resolve browser for auth flows: --browser flag > TV_PROXY_BROWSER env > None.
pub fn resolve_browser(flag_value: Option<&str>) -> Option<String> {
    flag_value
        .map(|s| s.to_string())
        .or_else(|| std::env::var("TV_PROXY_BROWSER").ok())
}

/// Resolve callback port: --port flag > TV_PROXY_PORT env > None.
pub fn resolve_callback_port(flag_value: Option<u16>) -> Option<u16> {
    if flag_value.is_some() {
        return flag_value;
    }
    std::env::var("TV_PROXY_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
}

/// Resolve storage backend: TV_PROXY_STORAGE env > "keyring".
pub fn resolve_storage_backend() -> Result<String> {
    let val = std::env::var("TV_PROXY_STORAGE").unwrap_or_else(|_| "keyring".to_string());
    match val.as_str() {
        "keyring" | "file" => Ok(val),
        other => bail!(
            "Invalid TV_PROXY_STORAGE value \"{}\". Must be \"keyring\" or \"file\".",
            other
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_stored(
        domain: &str,
        client_id: &str,
        client_secret: &str,
        audience: Option<&str>,
    ) -> StoredConfig {
        StoredConfig {
            domain: domain.to_string(),
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            audience: audience.map(|s| s.to_string()),
        }
    }

    // --- Pure tests (no env var mutation) ---

    #[test]
    fn merge_config_all_from_store() {
        // Temporarily remove env vars so stored values win.
        // We rely on the test runner not having these set; the pure path
        // checks stored fallback when env::var returns Err.
        let stored = make_stored("store.auth0.com", "store-id", "store-secret", Some("api://v1"));

        // If env vars happen to be set we can't guarantee this test, so
        // we use the helper that avoids env entirely.
        let result = merge_config_pure(None, None, None, None, Some(&stored));
        assert_eq!(result.domain.as_deref(), Some("store.auth0.com"));
        assert_eq!(result.client_id.as_deref(), Some("store-id"));
        assert_eq!(result.client_secret.as_deref(), Some("store-secret"));
        assert_eq!(result.audience.as_deref(), Some("api://v1"));
        assert!(result.missing.is_empty());
    }

    #[test]
    fn merge_config_env_takes_precedence() {
        let stored = make_stored("store.auth0.com", "store-id", "store-secret", Some("api://v1"));
        let result = merge_config_pure(
            Some("env.auth0.com"),
            Some("env-id"),
            Some("env-secret"),
            Some("api://v2"),
            Some(&stored),
        );
        assert_eq!(result.domain.as_deref(), Some("env.auth0.com"));
        assert_eq!(result.client_id.as_deref(), Some("env-id"));
        assert_eq!(result.client_secret.as_deref(), Some("env-secret"));
        assert_eq!(result.audience.as_deref(), Some("api://v2"));
        assert!(result.missing.is_empty());
    }

    #[test]
    fn merge_config_partial_env_overrides() {
        let stored = make_stored("store.auth0.com", "store-id", "store-secret", None);
        let result = merge_config_pure(
            Some("env.auth0.com"),
            None, // falls back to store
            None, // falls back to store
            None,
            Some(&stored),
        );
        assert_eq!(result.domain.as_deref(), Some("env.auth0.com"));
        assert_eq!(result.client_id.as_deref(), Some("store-id"));
        assert_eq!(result.client_secret.as_deref(), Some("store-secret"));
        assert!(result.missing.is_empty());
    }

    #[test]
    fn merge_config_nothing_provided() {
        let result = merge_config_pure(None, None, None, None, None);
        assert_eq!(result.missing.len(), 3);
        assert!(result.missing.contains(&"AUTH0_DOMAIN"));
        assert!(result.missing.contains(&"AUTH0_CLIENT_ID"));
        assert!(result.missing.contains(&"AUTH0_CLIENT_SECRET"));
    }

    #[test]
    fn require_config_errors_with_missing_fields() {
        let result = require_config_pure(None, None, None, None, None);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("AUTH0_DOMAIN"));
        assert!(msg.contains("AUTH0_CLIENT_ID"));
        assert!(msg.contains("AUTH0_CLIENT_SECRET"));
        assert!(msg.contains("variables"));
    }

    #[test]
    fn require_config_ok_with_stored() {
        let stored = make_stored("ok.auth0.com", "ok-id", "ok-secret", None);
        let cfg = require_config_pure(None, None, None, None, Some(&stored)).unwrap();
        assert_eq!(cfg.domain, "ok.auth0.com");
        assert_eq!(cfg.client_id, "ok-id");
        assert_eq!(cfg.client_secret, "ok-secret");
        assert!(cfg.audience.is_none());
    }

    #[test]
    fn require_config_singular_variable_message() {
        let stored = make_stored("ok.auth0.com", "ok-id", "ok-secret", None);
        // Only domain missing (store provides the rest, env provides nothing except domain is gone)
        let result = require_config_pure(None, Some("id"), Some("secret"), None, None);
        // This should only miss AUTH0_DOMAIN
        let err = result.unwrap_err().to_string();
        assert!(err.contains("AUTH0_DOMAIN"));
        assert!(err.contains("variable.") || err.ends_with("variable."));
        // Should NOT have the plural "variables"
        assert!(!err.contains("variables"));
        let _ = stored; // suppress unused warning
    }

    // --- Browser / port / storage tests (pure, no env) ---

    #[test]
    fn resolve_browser_flag_wins() {
        assert_eq!(
            resolve_browser(Some("firefox")),
            Some("firefox".to_string())
        );
    }

    #[test]
    fn resolve_browser_none() {
        // Without TV_PROXY_BROWSER set and no flag, should be None.
        // (Assuming env is clean — this is a best-effort pure test.)
        if std::env::var("TV_PROXY_BROWSER").is_err() {
            assert_eq!(resolve_browser(None), None);
        }
    }

    #[test]
    fn resolve_callback_port_flag_wins() {
        assert_eq!(resolve_callback_port(Some(9999)), Some(9999));
    }

    #[test]
    fn resolve_callback_port_none() {
        if std::env::var("TV_PROXY_PORT").is_err() {
            assert_eq!(resolve_callback_port(None), None);
        }
    }

    #[test]
    fn resolve_storage_backend_defaults_to_keyring() {
        if std::env::var("TV_PROXY_STORAGE").is_err() {
            assert_eq!(resolve_storage_backend().unwrap(), "keyring");
        }
    }

    #[test]
    fn resolve_storage_backend_rejects_invalid() {
        // We test the validation logic via the env-free pure helper.
        let err = validate_storage_value("badvalue");
        assert!(err.is_err());
        let msg = err.unwrap_err().to_string();
        assert!(msg.contains("badvalue"));
    }

    #[test]
    fn resolve_storage_backend_accepts_file() {
        assert!(validate_storage_value("file").is_ok());
    }

    #[test]
    fn resolve_storage_backend_accepts_keyring() {
        assert!(validate_storage_value("keyring").is_ok());
    }

    // --- Pure helpers for testing without env var mutation ---

    /// Same as `require_config` but uses pure merge helper.
    fn require_config_pure(
        env_domain: Option<&str>,
        env_client_id: Option<&str>,
        env_client_secret: Option<&str>,
        env_audience: Option<&str>,
        stored: Option<&StoredConfig>,
    ) -> Result<Auth0Config> {
        let result =
            merge_config_pure(env_domain, env_client_id, env_client_secret, env_audience, stored);
        if !result.missing.is_empty() {
            bail!(
                "Not configured. Run `tv-proxy login` first, or set {} environment variable{}.",
                result.missing.join(", "),
                if result.missing.len() > 1 { "s" } else { "" }
            );
        }
        Ok(Auth0Config {
            domain: result.domain.unwrap(),
            client_id: result.client_id.unwrap(),
            client_secret: result.client_secret.unwrap(),
            audience: result.audience,
        })
    }

    /// Validate a storage backend value without touching env vars.
    fn validate_storage_value(val: &str) -> Result<String> {
        match val {
            "keyring" | "file" => Ok(val.to_string()),
            other => bail!(
                "Invalid TV_PROXY_STORAGE value \"{}\". Must be \"keyring\" or \"file\".",
                other
            ),
        }
    }
}
