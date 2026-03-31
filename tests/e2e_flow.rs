mod e2e;

use e2e::fixture::{login, login_and_connect_gmail, parse_json, E2eFixture};

// ---------------------------------------------------------------------------
// Test 1: Full happy path — login → status → connect → connections → fetch → logout
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_full_happy_path() {
    let fixture = E2eFixture::setup().await;

    // Login
    login(&fixture);

    // Status shows logged in
    let status = fixture.run(&["--json", "status"]);
    assert_eq!(status.exit_code, 0);
    let json = parse_json(&status);
    assert_eq!(json["loggedIn"], true);
    assert_eq!(json["domain"], "test.auth0.com");
    assert_eq!(json["clientId"], "test-client-id");
    assert_eq!(json["storage"], "file");
    assert_eq!(json["tokenStatus"], "valid");

    // Connect gmail
    let connect = fixture.run(&["--json", "connect", "gmail"]);
    assert_eq!(connect.exit_code, 0, "connect stderr: {}", connect.stderr);
    let json = parse_json(&connect);
    assert_eq!(json["status"], "connected");
    assert_eq!(json["service"], "gmail");
    assert_eq!(json["connection"], "google-oauth2");

    // Connections shows the connected service
    let conns = fixture.run(&["--json", "connections"]);
    assert_eq!(conns.exit_code, 0);
    let json = parse_json(&conns);
    let connections = json["connections"]
        .as_array()
        .expect("connections should be array");
    assert!(
        !connections.is_empty(),
        "should have at least one connection"
    );
    assert_eq!(connections[0]["connection"], "google-oauth2");
    assert_eq!(connections[0]["tokenStatus"], "valid");
    assert_eq!(connections[0]["remote"], true);

    // Logout (--yes required in non-interactive mode)
    let logout = fixture.run(&["--json", "--yes", "logout", "--local"]);
    assert_eq!(logout.exit_code, 0);
    let json = parse_json(&logout);
    assert_eq!(json["status"], "logged_out");
}

// ---------------------------------------------------------------------------
// Test 2: Unauthenticated status, connections, logout
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_unauthenticated_responses() {
    let fixture = E2eFixture::setup().await;

    // Status when not logged in
    let status = fixture.run(&["--json", "status"]);
    assert_eq!(status.exit_code, 0);
    let json = parse_json(&status);
    assert_eq!(json["loggedIn"], false);
    assert_eq!(json["domain"], "test.auth0.com");
    assert_eq!(json["clientId"], "test-client-id");

    // Connections when not logged in
    let conns = fixture.run(&["--json", "connections"]);
    assert_eq!(conns.exit_code, 0);
    let json = parse_json(&conns);
    assert_eq!(json["connections"], serde_json::json!([]));

    // Logout when not logged in
    let logout = fixture.run(&["--json", "logout", "--local"]);
    assert_eq!(logout.exit_code, 0);
    let json = parse_json(&logout);
    assert_eq!(json["status"], "not_logged_in");
}

// ---------------------------------------------------------------------------
// Test 3: Fetch without connect → error
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_fetch_without_connect_errors() {
    let fixture = E2eFixture::setup().await;

    login(&fixture);

    // Fetch gmail without connecting first — token exchange should fail
    let result = fixture.run(&[
        "--json",
        "fetch",
        "gmail",
        "https://www.googleapis.com/gmail/v1/users/me/messages",
    ]);
    assert_ne!(result.exit_code, 0, "fetch should fail without connect");
    // Should contain an error in stdout (JSON mode) or stderr
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains("not authorized")
            || combined.contains("error")
            || combined.contains("auth"),
        "expected auth error, got stdout={} stderr={}",
        result.stdout,
        result.stderr
    );
}

// ---------------------------------------------------------------------------
// Test 4: Re-login with existing session
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_relogin_existing_session() {
    let fixture = E2eFixture::setup().await;

    login(&fixture);

    // Login again
    let result = fixture.run(&["--json", "login"]);
    assert_eq!(result.exit_code, 0);
    let json = parse_json(&result);
    assert_eq!(json["status"], "logged_in");
    assert_eq!(json["reauthenticated"], true);
}

// ---------------------------------------------------------------------------
// Test 5: Persists allowed domains and uses them for fetch
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_allowed_domains_for_fetch() {
    let fixture = E2eFixture::setup().await;

    // Login and connect with allowed domains pointing at our mock server
    let mock_hostname = extract_hostname(&fixture.mock.uri());
    let connect_json = login_and_connect_gmail(&fixture, &["--allowed-domains", &mock_hostname]);
    assert_eq!(connect_json["status"], "connected");
    assert_eq!(connect_json["service"], "gmail");

    // Mount an echo endpoint on the mock server
    mount_echo_endpoint(&fixture.mock.server).await;

    // Fetch to the echo endpoint on the mock server (HTTP allowed via TV_PROXY_ALLOW_HTTP)
    let echo_url = format!("{}/echo", fixture.mock.uri());
    let result = fixture.run(&["--json", "fetch", "gmail", &echo_url]);
    assert_eq!(
        result.exit_code, 0,
        "fetch failed: stdout={} stderr={}",
        result.stdout, result.stderr
    );
    let json = parse_json(&result);
    assert_eq!(json["status"], 200);
    assert_eq!(json["body"]["ok"], true);
    assert_eq!(json["body"]["method"], "GET");
    // Verify the token was sent in the Authorization header
    let auth_header = json["body"]["authorization"].as_str().unwrap_or("");
    assert!(
        auth_header.starts_with("Bearer "),
        "expected Bearer token in authorization header, got: {}",
        auth_header
    );

    // Verify settings were saved by running connections
    let conns = fixture.run(&["--json", "connections"]);
    assert_eq!(conns.exit_code, 0);
    let json = parse_json(&conns);
    let connections = json["connections"].as_array().expect("array");
    assert!(!connections.is_empty());
}

// ---------------------------------------------------------------------------
// Test 6: Rejects fetch to disallowed domains
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_fetch_disallowed_domain_rejected() {
    let fixture = E2eFixture::setup().await;

    login_and_connect_gmail(&fixture, &[]);

    // Fetch to a domain not in the allowed list
    let result = fixture.run(&["--json", "fetch", "gmail", "https://example.com/data"]);
    assert_ne!(result.exit_code, 0);
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains("not in the allowed list"),
        "expected domain rejection, got stdout={} stderr={}",
        result.stdout,
        result.stderr
    );
}

// ---------------------------------------------------------------------------
// Test 7: Local-only and remote disconnect flows
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_disconnect_flows() {
    let fixture = E2eFixture::setup().await;

    login_and_connect_gmail(&fixture, &[]);

    // Local disconnect — removes cached token, but remote still exists
    let local_disc = fixture.run(&["--json", "--yes", "disconnect", "gmail"]);
    assert_eq!(local_disc.exit_code, 0);
    let json = parse_json(&local_disc);
    assert_eq!(json["status"], "disconnected");
    assert_eq!(json["service"], "gmail");
    assert_eq!(json["remote"], false);

    // Connections should still show the remote account (without local token)
    let conns = fixture.run(&["--json", "connections"]);
    assert_eq!(conns.exit_code, 0);
    let json = parse_json(&conns);
    let connections = json["connections"].as_array().expect("array");
    assert!(
        !connections.is_empty(),
        "remote connection should still exist"
    );
    assert_eq!(connections[0]["connection"], "google-oauth2");
    assert_eq!(connections[0]["tokenStatus"], "none");
    assert_eq!(connections[0]["remote"], true);

    // Remote disconnect — removes from server too
    let remote_disc = fixture.run(&["--json", "--yes", "disconnect", "gmail", "--remote"]);
    assert_eq!(remote_disc.exit_code, 0);
    let json = parse_json(&remote_disc);
    assert_eq!(json["status"], "disconnected");
    assert_eq!(json["service"], "gmail");
    assert_eq!(json["remote"], true);

    // Connections should now be empty
    let final_conns = fixture.run(&["--json", "connections"]);
    assert_eq!(final_conns.exit_code, 0);
    let json = parse_json(&final_conns);
    assert_eq!(json["connections"], serde_json::json!([]));
}

// ---------------------------------------------------------------------------
// Test 8: Remote disconnect without login → auth_required
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_remote_disconnect_requires_login() {
    let fixture = E2eFixture::setup().await;

    let result = fixture.run(&["--json", "--yes", "disconnect", "gmail", "--remote"]);
    assert_ne!(result.exit_code, 0);
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains("Not logged in") || combined.contains("auth_required"),
        "expected auth_required error, got stdout={} stderr={}",
        result.stdout,
        result.stderr
    );
}

// ---------------------------------------------------------------------------
// Test 9: Invalid service errors for connect/disconnect/fetch
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_invalid_service_errors() {
    let fixture = E2eFixture::setup().await;

    // Connect unknown service
    let result = fixture.run(&["--json", "connect", "not-a-service"]);
    assert_ne!(result.exit_code, 0);
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains("Unknown") || combined.contains("invalid"),
        "connect: expected unknown service error, got stdout={} stderr={}",
        result.stdout,
        result.stderr
    );

    // Disconnect unknown service
    let result = fixture.run(&["--json", "disconnect", "not-a-service"]);
    assert_ne!(result.exit_code, 0);
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains("Unknown") || combined.contains("invalid"),
        "disconnect: expected unknown service error, got stdout={} stderr={}",
        result.stdout,
        result.stderr
    );

    // Fetch unknown service
    let result = fixture.run(&[
        "--json",
        "fetch",
        "not-a-service",
        "https://example.com/data",
    ]);
    assert_ne!(result.exit_code, 0);
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains("Unknown") || combined.contains("invalid"),
        "fetch: expected unknown service error, got stdout={} stderr={}",
        result.stdout,
        result.stderr
    );
}

// ---------------------------------------------------------------------------
// Test 10: Config preserved after local logout
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_config_preserved_after_logout() {
    let fixture = E2eFixture::setup().await;

    login_and_connect_gmail(&fixture, &[]);

    // Local logout
    let logout = fixture.run(&["--json", "--yes", "logout", "--local"]);
    assert_eq!(logout.exit_code, 0);

    // Status should still show domain/clientId despite being logged out
    let status = fixture.run(&["--json", "status"]);
    assert_eq!(status.exit_code, 0);
    let json = parse_json(&status);
    assert_eq!(json["loggedIn"], false);
    assert_eq!(json["domain"], "test.auth0.com");
    assert_eq!(json["clientId"], "test-client-id");
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn extract_host(uri: &str) -> String {
    // e.g. "http://127.0.0.1:12345" → "127.0.0.1:12345"
    uri.replacen("http://", "", 1).replacen("https://", "", 1)
}

fn extract_hostname(uri: &str) -> String {
    // e.g. "http://127.0.0.1:12345" → "127.0.0.1"
    let host = extract_host(uri);
    host.split(':').next().unwrap_or(&host).to_string()
}

async fn mount_echo_endpoint(server: &wiremock::MockServer) {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, Request, ResponseTemplate};

    Mock::given(method("GET"))
        .and(path("/echo"))
        .respond_with(|req: &Request| {
            let auth = req
                .headers
                .get("Authorization")
                .map(|v| v.to_str().unwrap_or(""))
                .unwrap_or("");
            ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "ok": true,
                "method": "GET",
                "authorization": auth,
            }))
        })
        .mount(server)
        .await;
}

// ---------------------------------------------------------------------------
// Test 11: Fetch unconnected service → exit 4 (authz_required)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_fetch_unconnected_service_returns_exit_4() {
    let fixture = E2eFixture::setup().await;
    login(&fixture);

    // Fetch gmail without connecting — mock returns access_denied (403)
    let result = fixture.run(&[
        "--json",
        "fetch",
        "gmail",
        "https://www.googleapis.com/gmail/v1/users/me/messages",
    ]);
    assert_eq!(
        result.exit_code, 4,
        "expected exit code 4 (authz_required), got {}: stderr={}",
        result.exit_code, result.stderr
    );
}

// ---------------------------------------------------------------------------
// Test 12: Fetch with forced token error → exit 3 (auth_required)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_fetch_expired_token_returns_exit_3() {
    let fixture = E2eFixture::setup().await;

    // Login and connect first
    login_and_connect_gmail(&fixture, &[]);

    // Force token exchange to return invalid_grant
    let force_error = serde_json::json!({
        "error": "invalid_grant",
        "error_description": "Token has been revoked",
        "status": 400,
    });
    std::fs::write(
        fixture.mock.state_dir.join("e2e-force-token-error.json"),
        serde_json::to_string(&force_error).unwrap(),
    )
    .unwrap();

    // Clear cached connection token so it goes to exchange
    let disconnect = fixture.run(&["--json", "--yes", "disconnect", "gmail"]);
    assert_eq!(disconnect.exit_code, 0);

    // Re-connect gmail so we have the account registered
    let _connect = fixture.run(&["--json", "connect", "gmail"]);
    // Connect does the token exchange itself; it should warn but succeed
    // The fetch below is the one that should fail
    // Remove force error, re-apply after connect
    let _ = std::fs::remove_file(fixture.mock.state_dir.join("e2e-force-token-error.json"));

    login_and_connect_gmail(&fixture, &[]);

    // Now force the error again for the fetch
    std::fs::write(
        fixture.mock.state_dir.join("e2e-force-token-error.json"),
        serde_json::to_string(&force_error).unwrap(),
    )
    .unwrap();

    // Remove local cached connection token to force exchange
    let disc = fixture.run(&["--json", "--yes", "disconnect", "gmail"]);
    assert_eq!(disc.exit_code, 0);

    // Fetch should try to exchange → get invalid_grant → exit 3
    let result = fixture.run(&[
        "--json",
        "fetch",
        "gmail",
        "https://www.googleapis.com/gmail/v1/users/me/messages",
    ]);
    assert_eq!(
        result.exit_code, 3,
        "expected exit code 3 (auth_required), got {}: stdout={} stderr={}",
        result.exit_code, result.stdout, result.stderr
    );
}

// ---------------------------------------------------------------------------
// Test 13: Fetch to error endpoint → exit 5 (service_error)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_fetch_non_2xx_returns_exit_5() {
    let fixture = E2eFixture::setup().await;
    let mock_hostname = extract_hostname(&fixture.mock.uri());
    login_and_connect_gmail(&fixture, &["--allowed-domains", &mock_hostname]);

    let error_url = format!("{}/echo/status/500", fixture.mock.uri());
    let result = fixture.run(&["--json", "fetch", "gmail", &error_url]);
    assert_eq!(
        result.exit_code, 5,
        "expected exit code 5 (service_error), got {}: stdout={} stderr={}",
        result.exit_code, result.stdout, result.stderr
    );

    // stdout has two JSON objects (response + error); parse the first one
    let first_json: serde_json::Value = serde_json::Deserializer::from_str(&result.stdout)
        .into_iter()
        .next()
        .expect("should have at least one JSON object")
        .expect("should parse first JSON");
    assert_eq!(first_json["status"], 500);
}

// ---------------------------------------------------------------------------
// Test 14: Fetch custom headers via -H
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_fetch_custom_headers() {
    let fixture = E2eFixture::setup().await;
    let mock_hostname = extract_hostname(&fixture.mock.uri());
    login_and_connect_gmail(&fixture, &["--allowed-domains", &mock_hostname]);

    let echo_url = format!("{}/echo", fixture.mock.uri());
    let result = fixture.run(&[
        "--json",
        "fetch",
        "gmail",
        &echo_url,
        "-H",
        "X-Custom-Header: test-value-123",
        "-H",
        "X-Another: hello",
    ]);
    assert_eq!(
        result.exit_code, 0,
        "fetch failed: stdout={} stderr={}",
        result.stdout, result.stderr
    );
    let json = parse_json(&result);
    assert_eq!(json["body"]["headers"]["x-custom-header"], "test-value-123");
    assert_eq!(json["body"]["headers"]["x-another"], "hello");
}

// ---------------------------------------------------------------------------
// Test 15: Fetch POST with -d body
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_fetch_post_with_body() {
    let fixture = E2eFixture::setup().await;
    let mock_hostname = extract_hostname(&fixture.mock.uri());
    login_and_connect_gmail(&fixture, &["--allowed-domains", &mock_hostname]);

    let echo_url = format!("{}/echo", fixture.mock.uri());
    let result = fixture.run(&[
        "--json",
        "fetch",
        "gmail",
        &echo_url,
        "-X",
        "POST",
        "-d",
        r#"{"key":"value"}"#,
    ]);
    assert_eq!(
        result.exit_code, 0,
        "fetch failed: stdout={} stderr={}",
        result.stdout, result.stderr
    );
    let json = parse_json(&result);
    assert_eq!(json["body"]["method"], "POST");
    assert_eq!(json["body"]["body"], r#"{"key":"value"}"#);
}

// ---------------------------------------------------------------------------
// Test 16: Fetch POST with --data-file
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_fetch_post_with_data_file() {
    let fixture = E2eFixture::setup().await;
    let mock_hostname = extract_hostname(&fixture.mock.uri());
    login_and_connect_gmail(&fixture, &["--allowed-domains", &mock_hostname]);

    // Write a temp data file
    let data_file = fixture.temp_dir.path().join("request-body.json");
    std::fs::write(&data_file, r#"{"from_file":true}"#).unwrap();

    let echo_url = format!("{}/echo", fixture.mock.uri());
    let result = fixture.run(&[
        "--json",
        "fetch",
        "gmail",
        &echo_url,
        "-X",
        "POST",
        "--data-file",
        data_file.to_str().unwrap(),
    ]);
    assert_eq!(
        result.exit_code, 0,
        "fetch failed: stdout={} stderr={}",
        result.stdout, result.stderr
    );
    let json = parse_json(&result);
    assert_eq!(json["body"]["method"], "POST");
    assert_eq!(json["body"]["body"], r#"{"from_file":true}"#);
}

// ---------------------------------------------------------------------------
// Test 17: Disconnect --remote for non-existent account → warning, exit 0
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_disconnect_remote_nonexistent_is_warning() {
    let fixture = E2eFixture::setup().await;
    login(&fixture);

    // Disconnect remote for gmail (never connected) — should succeed with warning
    let result = fixture.run(&["--json", "--yes", "disconnect", "gmail", "--remote"]);
    assert_eq!(
        result.exit_code, 0,
        "expected exit code 0 (warning only), got {}: stderr={}",
        result.exit_code, result.stderr
    );

    // Output should contain a warning about no remote connection
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains("warning")
            || combined.contains("Warning")
            || combined.contains("No remote connection"),
        "expected warning about missing remote connection, got stdout={} stderr={}",
        result.stdout,
        result.stderr
    );
}

// ---------------------------------------------------------------------------
// Test 18: Connections falls back to local when remote API fails
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_connections_fallback_to_local() {
    let fixture = E2eFixture::setup().await;
    login_and_connect_gmail(&fixture, &[]);

    // Verify connections work normally first
    let conns = fixture.run(&["--json", "connections"]);
    assert_eq!(conns.exit_code, 0);
    let json = parse_json(&conns);
    let connections = json["connections"].as_array().expect("array");
    assert!(!connections.is_empty());

    // Force remote list API to fail
    std::fs::write(
        fixture.mock.state_dir.join("e2e-force-list-error.json"),
        "trigger",
    )
    .unwrap();

    // Connections should fall back to local data
    let conns = fixture.run(&["--json", "connections"]);
    assert_eq!(
        conns.exit_code, 0,
        "connections should succeed even when remote fails: stderr={}",
        conns.stderr
    );
    let json = parse_json(&conns);
    let connections = json["connections"].as_array().expect("array");
    // Should still have at least the locally cached connection
    assert!(
        !connections.is_empty(),
        "should have local connections as fallback"
    );
}
