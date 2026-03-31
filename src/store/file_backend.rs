use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tracing::debug;

use super::backend::CredentialBackend;
use super::types::{
    Auth0Tokens, ConnectionToken, CredentialData, ServiceSettings, StoredConfig,
};

const DEFAULT_DIR_NAME: &str = ".tv-proxy";
const CREDENTIALS_FILE: &str = "credentials.json";

/// Resolve the config directory from `TV_PROXY_CONFIG_DIR` or default to `~/.tv-proxy/`.
fn resolve_config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("TV_PROXY_CONFIG_DIR") {
        PathBuf::from(dir)
    } else {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(DEFAULT_DIR_NAME)
    }
}

/// File-based credential backend.
///
/// Stores all credential data in a single JSON file at `{dir}/credentials.json`.
/// Uses atomic writes (write to temp file, then rename) and restricts file
/// permissions to 0600 on Unix.
pub struct FileBackend {
    dir: PathBuf,
    file_path: PathBuf,
}

impl FileBackend {
    /// Create a `FileBackend` using the default config directory.
    pub fn new() -> Self {
        Self::with_dir(resolve_config_dir())
    }

    /// Create a `FileBackend` with an explicit directory (useful for testing).
    pub fn with_dir(dir: PathBuf) -> Self {
        let file_path = dir.join(CREDENTIALS_FILE);
        Self { dir, file_path }
    }

    /// Load credential data from disk.
    ///
    /// Returns `CredentialData::default()` if the file does not exist.
    /// Propagates all other IO and parse errors (institutional learning #4).
    fn load(&self) -> Result<CredentialData> {
        match fs::read_to_string(&self.file_path) {
            Ok(raw) => {
                let data: CredentialData = serde_json::from_str(&raw)
                    .with_context(|| format!("failed to parse {}", self.file_path.display()))?;
                Ok(data)
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                debug!("credentials file not found, returning defaults");
                Ok(CredentialData::default())
            }
            Err(err) => Err(err)
                .with_context(|| format!("failed to read {}", self.file_path.display())),
        }
    }

    /// Persist credential data to disk atomically.
    ///
    /// 1. Ensure directory exists with 0700 permissions.
    /// 2. Write to a temporary file in the same directory.
    /// 3. Rename temp file to the target path (atomic on POSIX).
    /// 4. Set file permissions to 0600.
    fn persist(&self, data: &CredentialData) -> Result<()> {
        self.ensure_dir()?;

        let json = serde_json::to_string_pretty(data)
            .context("failed to serialize credential data")?;

        // Write to a temp file alongside the target for same-filesystem rename.
        // On Unix, create with mode 0o600 directly to avoid a TOCTOU window
        // where the file briefly has default permissions.
        let temp_path = self.dir.join(".credentials.json.tmp");
        write_with_restricted_permissions(&temp_path, json.as_bytes())?;

        fs::rename(&temp_path, &self.file_path)
            .with_context(|| {
                format!(
                    "failed to rename {} to {}",
                    temp_path.display(),
                    self.file_path.display()
                )
            })?;

        debug!("credentials persisted to {}", self.file_path.display());
        Ok(())
    }

    /// Ensure the config directory exists with 0700 permissions.
    fn ensure_dir(&self) -> Result<()> {
        if !self.dir.exists() {
            fs::create_dir_all(&self.dir)
                .with_context(|| format!("failed to create directory {}", self.dir.display()))?;
            set_dir_permissions(&self.dir)?;
        }
        Ok(())
    }
}

impl CredentialBackend for FileBackend {
    fn get_config(&self) -> Result<Option<StoredConfig>> {
        Ok(self.load()?.config)
    }

    fn save_config(&self, config: &StoredConfig) -> Result<()> {
        let mut data = self.load()?;
        data.config = Some(config.clone());
        self.persist(&data)
    }

    fn get_auth0_tokens(&self) -> Result<Option<Auth0Tokens>> {
        Ok(self.load()?.auth0)
    }

    fn save_auth0_tokens(&self, tokens: &Auth0Tokens) -> Result<()> {
        let mut data = self.load()?;
        data.auth0 = Some(tokens.clone());
        self.persist(&data)
    }

    fn get_connection_token(&self, connection: &str) -> Result<Option<ConnectionToken>> {
        Ok(self.load()?.connections.get(connection).cloned())
    }

    fn save_connection_token(&self, connection: &str, token: &ConnectionToken) -> Result<()> {
        let mut data = self.load()?;
        data.connections.insert(connection.to_string(), token.clone());
        self.persist(&data)
    }

    fn list_connections(&self) -> Result<Vec<String>> {
        let data = self.load()?;
        Ok(data.connections.keys().cloned().collect())
    }

    fn remove_connection(&self, connection: &str) -> Result<()> {
        let mut data = self.load()?;
        data.connections.remove(connection);
        self.persist(&data)
    }

    fn get_service_settings(&self, service: &str) -> Result<Option<ServiceSettings>> {
        let data = self.load()?;
        Ok(data
            .service_settings
            .as_ref()
            .and_then(|m| m.get(service).cloned()))
    }

    fn save_service_settings(&self, service: &str, settings: &ServiceSettings) -> Result<()> {
        let mut data = self.load()?;
        data.service_settings
            .get_or_insert_with(HashMap::new)
            .insert(service.to_string(), settings.clone());
        self.persist(&data)
    }

    fn clear(&self) -> Result<()> {
        let data = self.load()?;

        if data.config.is_some() || data.service_settings.is_some() {
            // Preserve config and service_settings, wipe auth0 and connections
            let cleared = CredentialData {
                config: data.config,
                auth0: None,
                connections: HashMap::new(),
                service_settings: data.service_settings,
            };
            self.persist(&cleared)
        } else {
            // Nothing to preserve — delete the file
            match fs::remove_file(&self.file_path) {
                Ok(()) => Ok(()),
                Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(err) => Err(err)
                    .with_context(|| format!("failed to remove {}", self.file_path.display())),
            }
        }
    }
}

/// Write data to a file, creating it with mode 0o600 atomically on Unix.
/// On non-Unix platforms, falls back to regular write + post-hoc chmod.
#[cfg(unix)]
fn write_with_restricted_permissions(path: &Path, data: &[u8]) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    use std::io::Write;

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("failed to create {}", path.display()))?;
    file.write_all(data)
        .with_context(|| format!("failed to write {}", path.display()))
}

#[cfg(not(unix))]
fn write_with_restricted_permissions(path: &Path, data: &[u8]) -> Result<()> {
    fs::write(path, data)
        .with_context(|| format!("failed to write {}", path.display()))
}

/// Set directory permissions to 0700 (owner only) on Unix.
#[cfg(unix)]
fn set_dir_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("failed to set directory permissions on {}", path.display()))
}

/// No-op on non-Unix platforms.
#[cfg(not(unix))]
fn set_dir_permissions(_path: &Path) -> Result<()> {
    Ok(())
}
