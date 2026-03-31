use std::path::PathBuf;
use std::process::Command;

use super::mock_server::MockAuth0Server;

pub struct CliResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

pub struct E2eFixture {
    pub temp_dir: tempfile::TempDir,
    pub mock: MockAuth0Server,
    fake_browser: PathBuf,
}

impl E2eFixture {
    pub async fn setup() -> Self {
        let temp_dir = tempfile::TempDir::new().expect("failed to create temp dir");
        let mock = MockAuth0Server::start(temp_dir.path()).await;

        // Locate the fake browser script relative to the test binary
        let fake_browser = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/e2e/fake_browser.sh");

        assert!(fake_browser.exists(), "fake_browser.sh not found at {:?}", fake_browser);

        Self {
            temp_dir,
            mock,
            fake_browser,
        }
    }

    pub fn run(&self, args: &[&str]) -> CliResult {
        let output = Command::new(assert_cmd::cargo::cargo_bin("tv-proxy"))
            .args(args)
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .env("HOME", std::env::var("HOME").unwrap_or_default())
            .env("TV_PROXY_AUTH0_BASE_URL", self.mock.uri())
            .env("AUTH0_DOMAIN", "test.auth0.com")
            .env("AUTH0_CLIENT_ID", "test-client-id")
            .env("AUTH0_CLIENT_SECRET", "test-client-secret")
            .env("TV_PROXY_STORAGE", "file")
            .env("TV_PROXY_CONFIG_DIR", self.temp_dir.path())
            .env("TV_PROXY_BROWSER", &self.fake_browser)
            .env("TV_PROXY_PORT", "0")
            .env("TV_PROXY_ALLOW_HTTP", "1")
            .env("NO_COLOR", "1")
            .output()
            .expect("failed to execute tv-proxy");

        CliResult {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        }
    }
}

pub fn parse_json(result: &CliResult) -> serde_json::Value {
    serde_json::from_str(&result.stdout)
        .unwrap_or_else(|e| panic!("Failed to parse JSON from stdout: {}\nstdout: {}\nstderr: {}", e, result.stdout, result.stderr))
}

/// Helper: login and assert success
pub fn login(fixture: &E2eFixture) -> CliResult {
    let result = fixture.run(&["--json", "login"]);
    assert_eq!(result.exit_code, 0, "login failed: stderr={}", result.stderr);
    let json = parse_json(&result);
    assert_eq!(json["status"], "logged_in", "unexpected login response: {}", result.stdout);
    result
}

/// Helper: login then connect gmail, return the connect JSON
pub fn login_and_connect_gmail(fixture: &E2eFixture, extra_args: &[&str]) -> serde_json::Value {
    login(fixture);
    let mut args = vec!["--json", "connect", "gmail"];
    args.extend_from_slice(extra_args);
    let result = fixture.run(&args);
    assert_eq!(result.exit_code, 0, "connect failed: stderr={}", result.stderr);
    parse_json(&result)
}
