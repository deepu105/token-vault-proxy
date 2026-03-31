use assert_cmd::Command;
use predicates::prelude::*;

fn tv_proxy() -> Command {
    Command::cargo_bin("tv-proxy").unwrap()
}

// ---------------------------------------------------------------------------
// CLI basics
// ---------------------------------------------------------------------------

#[test]
fn help_flag_shows_all_commands() {
    tv_proxy()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("login"))
        .stdout(predicate::str::contains("logout"))
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("connect"))
        .stdout(predicate::str::contains("disconnect"))
        .stdout(predicate::str::contains("connections"))
        .stdout(predicate::str::contains("fetch"))
        .stdout(predicate::str::contains("init"));
}

#[test]
fn version_flag_prints_version() {
    tv_proxy()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("tv-proxy"));
}

#[test]
fn unknown_command_fails() {
    tv_proxy()
        .arg("nonexistent")
        .assert()
        .failure();
}

// ---------------------------------------------------------------------------
// Status without config (should succeed with "not logged in")
// ---------------------------------------------------------------------------

#[test]
fn status_without_config_shows_not_logged_in() {
    tv_proxy()
        .arg("status")
        .env("TV_PROXY_STORAGE", "file")
        .env("TV_PROXY_CONFIG_DIR", tempfile::TempDir::new().unwrap().path().as_os_str())
        .env_remove("AUTH0_DOMAIN")
        .env_remove("AUTH0_CLIENT_ID")
        .env_remove("AUTH0_CLIENT_SECRET")
        .assert()
        .success()
        .stdout(predicate::str::contains("Not logged in"));
}

#[test]
fn status_json_without_config() {
    tv_proxy()
        .args(["status", "--json"])
        .env("TV_PROXY_STORAGE", "file")
        .env("TV_PROXY_CONFIG_DIR", tempfile::TempDir::new().unwrap().path().as_os_str())
        .env_remove("AUTH0_DOMAIN")
        .env_remove("AUTH0_CLIENT_ID")
        .env_remove("AUTH0_CLIENT_SECRET")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"loggedIn\": false"));
}

// ---------------------------------------------------------------------------
// Login without config (should fail with config error)
// ---------------------------------------------------------------------------

#[test]
fn login_without_config_errors() {
    tv_proxy()
        .arg("login")
        .env("TV_PROXY_STORAGE", "file")
        .env("TV_PROXY_CONFIG_DIR", tempfile::TempDir::new().unwrap().path().as_os_str())
        .env_remove("AUTH0_DOMAIN")
        .env_remove("AUTH0_CLIENT_ID")
        .env_remove("AUTH0_CLIENT_SECRET")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Not configured"));
}

#[test]
fn login_without_config_json_errors() {
    tv_proxy()
        .args(["login", "--json"])
        .env("TV_PROXY_STORAGE", "file")
        .env("TV_PROXY_CONFIG_DIR", tempfile::TempDir::new().unwrap().path().as_os_str())
        .env_remove("AUTH0_DOMAIN")
        .env_remove("AUTH0_CLIENT_ID")
        .env_remove("AUTH0_CLIENT_SECRET")
        .assert()
        .failure()
        .stdout(predicate::str::contains("\"code\""))
        .stdout(predicate::str::contains("Not configured"));
}

// ---------------------------------------------------------------------------
// Connections without login
// ---------------------------------------------------------------------------

#[test]
fn connections_empty_shows_no_services() {
    tv_proxy()
        .arg("connections")
        .env("TV_PROXY_STORAGE", "file")
        .env("TV_PROXY_CONFIG_DIR", tempfile::TempDir::new().unwrap().path().as_os_str())
        .env_remove("AUTH0_DOMAIN")
        .env_remove("AUTH0_CLIENT_ID")
        .env_remove("AUTH0_CLIENT_SECRET")
        .assert()
        .success()
        .stdout(predicate::str::contains("No services connected"));
}

#[test]
fn connections_json_returns_empty_array() {
    tv_proxy()
        .args(["connections", "--json"])
        .env("TV_PROXY_STORAGE", "file")
        .env("TV_PROXY_CONFIG_DIR", tempfile::TempDir::new().unwrap().path().as_os_str())
        .env_remove("AUTH0_DOMAIN")
        .env_remove("AUTH0_CLIENT_ID")
        .env_remove("AUTH0_CLIENT_SECRET")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"connections\": []"));
}

// ---------------------------------------------------------------------------
// Connect without login (should error with auth_required)
// ---------------------------------------------------------------------------

#[test]
fn connect_without_login_errors() {
    tv_proxy()
        .args(["connect", "gmail"])
        .env("TV_PROXY_STORAGE", "file")
        .env("TV_PROXY_CONFIG_DIR", tempfile::TempDir::new().unwrap().path().as_os_str())
        .env("AUTH0_DOMAIN", "test.auth0.com")
        .env("AUTH0_CLIENT_ID", "test-id")
        .env("AUTH0_CLIENT_SECRET", "test-secret")
        .assert()
        .failure();
}

// ---------------------------------------------------------------------------
// Fetch validation
// ---------------------------------------------------------------------------

#[test]
fn fetch_unknown_service_errors() {
    tv_proxy()
        .args(["fetch", "nonexistent", "https://example.com"])
        .env("TV_PROXY_STORAGE", "file")
        .env("TV_PROXY_CONFIG_DIR", tempfile::TempDir::new().unwrap().path().as_os_str())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Unknown service"));
}

#[test]
fn fetch_http_url_rejected() {
    tv_proxy()
        .args(["fetch", "gmail", "http://insecure.example.com"])
        .env("TV_PROXY_STORAGE", "file")
        .env("TV_PROXY_CONFIG_DIR", tempfile::TempDir::new().unwrap().path().as_os_str())
        .assert()
        .failure()
        .stderr(predicate::str::contains("HTTPS"));
}

#[test]
fn fetch_disallowed_domain_rejected() {
    tv_proxy()
        .args(["fetch", "gmail", "https://evil.example.com/data"])
        .env("TV_PROXY_STORAGE", "file")
        .env("TV_PROXY_CONFIG_DIR", tempfile::TempDir::new().unwrap().path().as_os_str())
        .assert()
        .failure()
        .stderr(predicate::str::contains("not in the allowed list"));
}

#[test]
fn fetch_json_unknown_service() {
    tv_proxy()
        .args(["fetch", "--json", "nonexistent", "https://example.com"])
        .env("TV_PROXY_STORAGE", "file")
        .env("TV_PROXY_CONFIG_DIR", tempfile::TempDir::new().unwrap().path().as_os_str())
        .assert()
        .failure()
        .stdout(predicate::str::contains("Unknown service"));
}

// ---------------------------------------------------------------------------
// Disconnect unknown service
// ---------------------------------------------------------------------------

#[test]
fn disconnect_unknown_service_errors() {
    tv_proxy()
        .args(["disconnect", "nonexistent"])
        .env("TV_PROXY_STORAGE", "file")
        .env("TV_PROXY_CONFIG_DIR", tempfile::TempDir::new().unwrap().path().as_os_str())
        .assert()
        .failure()
        .stderr(predicate::str::contains("Unknown provider"));
}

// ---------------------------------------------------------------------------
// Logout without login (idempotent)
// ---------------------------------------------------------------------------

#[test]
fn logout_when_not_logged_in() {
    tv_proxy()
        .arg("logout")
        .env("TV_PROXY_STORAGE", "file")
        .env("TV_PROXY_CONFIG_DIR", tempfile::TempDir::new().unwrap().path().as_os_str())
        .env_remove("AUTH0_DOMAIN")
        .env_remove("AUTH0_CLIENT_ID")
        .env_remove("AUTH0_CLIENT_SECRET")
        .assert()
        .success()
        .stdout(predicate::str::contains("Not logged in"));
}
