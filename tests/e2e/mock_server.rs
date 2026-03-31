use std::path::{Path, PathBuf};
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// A mock Auth0 server backed by wiremock.
/// Stateful connected accounts are persisted to a JSON file in `state_dir`.
pub struct MockAuth0Server {
    pub server: MockServer,
    pub state_dir: PathBuf,
}

impl MockAuth0Server {
    /// Start the mock server and mount all Auth0 endpoint handlers.
    pub async fn start(state_dir: &Path) -> Self {
        let server = MockServer::start().await;
        let state_dir = state_dir.to_path_buf();

        // --- OIDC Discovery ---
        let uri = server.uri();
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(move |_req: &Request| {
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "issuer": format!("{}/", uri),
                    "authorization_endpoint": format!("{}/authorize", uri),
                    "token_endpoint": format!("{}/oauth/token", uri),
                    "userinfo_endpoint": format!("{}/userinfo", uri),
                    "jwks_uri": format!("{}/.well-known/jwks.json", uri),
                    "response_types_supported": ["code"],
                    "subject_types_supported": ["public"],
                    "id_token_signing_alg_values_supported": ["RS256"],
                    "token_endpoint_auth_methods_supported": ["client_secret_post", "client_secret_basic"],
                    "code_challenge_methods_supported": ["S256"],
                }))
            })
            .mount(&server)
            .await;

        // --- Token endpoint (handles multiple grant types) ---
        let sd = state_dir.clone();
        let _server_uri = server.uri();
        Mock::given(method("POST"))
            .and(path("/oauth/token"))
            .respond_with(move |req: &Request| {
                let body = String::from_utf8(req.body.clone()).unwrap_or_default();
                let params: Vec<(String, String)> = url::form_urlencoded::parse(body.as_bytes())
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
                let get = |key: &str| -> String {
                    params.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()).unwrap_or_default()
                };

                let grant_type = get("grant_type");

                if grant_type == "authorization_code" {
                    // Build a mock ID token (base64-encoded JSON, no signature)
                    let header = base64_url_encode(r#"{"alg":"RS256","typ":"JWT"}"#);
                    let payload = base64_url_encode(r#"{"sub":"auth0|user123","email":"test@example.com","name":"Test User","iss":"https://test.auth0.com/","aud":"test-client-id","exp":9999999999}"#);
                    let id_token = format!("{}.{}.fake-signature", header, payload);

                    return ResponseTemplate::new(200).set_body_json(serde_json::json!({
                        "access_token": "mock-access-token",
                        "refresh_token": "mock-refresh-token",
                        "id_token": id_token,
                        "expires_in": 86400,
                        "token_type": "Bearer",
                        "scope": "openid profile email offline_access",
                    }));
                }

                if grant_type == "refresh_token" {
                    let audience = get("audience");
                    if audience.contains("/me/") {
                        return ResponseTemplate::new(200).set_body_json(serde_json::json!({
                            "access_token": "mock-my-account-token",
                            "expires_in": 3600,
                            "token_type": "Bearer",
                            "scope": "create:me:connected_accounts read:me:connected_accounts delete:me:connected_accounts",
                        }));
                    }

                    return ResponseTemplate::new(200).set_body_json(serde_json::json!({
                        "access_token": "refreshed-access-token",
                        "expires_in": 86400,
                        "token_type": "Bearer",
                    }));
                }

                if grant_type.contains("federated-connection-access-token") {
                    let connection = get("connection");
                    let accounts = load_accounts(&sd);
                    let authorized = accounts.iter().any(|a| {
                        a.get("connection").and_then(|v| v.as_str()) == Some(&connection)
                    });

                    if !authorized {
                        return ResponseTemplate::new(403).set_body_json(serde_json::json!({
                            "error": "access_denied",
                            "error_description": format!("Connection {} is not authorized", connection),
                        }));
                    }

                    return ResponseTemplate::new(200).set_body_json(serde_json::json!({
                        "access_token": "mock-gmail-access-token",
                        "expires_in": 3600,
                        "token_type": "Bearer",
                        "scope": "https://www.googleapis.com/auth/gmail.modify",
                    }));
                }

                ResponseTemplate::new(400).set_body_json(serde_json::json!({
                    "error": "unsupported_grant_type",
                    "error_description": "Unsupported grant type",
                }))
            })
            .mount(&server)
            .await;

        // --- Connected Accounts: Initiate Connect ---
        let sd = state_dir.clone();
        let su = server.uri();
        Mock::given(method("POST"))
            .and(path("/me/v1/connected-accounts/connect"))
            .respond_with(move |req: &Request| {
                let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or_default();
                let redirect_uri = body.get("redirect_uri").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let state = body.get("state").and_then(|v| v.as_str()).unwrap_or("").to_string();

                // Write connect callback details to state file for fake browser to read
                let connect_state = serde_json::json!({
                    "redirect_uri": redirect_uri,
                    "state": state,
                });
                let connect_state_file = sd.join("e2e-connect-state.json");
                let _ = std::fs::write(&connect_state_file, serde_json::to_string(&connect_state).unwrap());

                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "auth_session": "mock-auth-session-123",
                    "connect_uri": format!("{}/connected-accounts/connect", su),
                    "connect_params": { "ticket": "mock-ticket" },
                    "expires_in": 300,
                }))
            })
            .mount(&server)
            .await;

        // --- Connected Accounts: Complete ---
        let sd = state_dir.clone();
        Mock::given(method("POST"))
            .and(path("/me/v1/connected-accounts/complete"))
            .respond_with(move |_req: &Request| {
                let mut accounts = load_accounts(&sd);
                let existing = accounts.iter().any(|a| {
                    a.get("connection").and_then(|v| v.as_str()) == Some("google-oauth2")
                });
                if !existing {
                    accounts.push(serde_json::json!({
                        "id": "ca_abc123",
                        "connection": "google-oauth2",
                        "scopes": ["https://www.googleapis.com/auth/gmail.modify"],
                    }));
                    save_accounts(&sd, &accounts);
                }

                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "id": "ca_abc123",
                    "connection": "google-oauth2",
                    "scopes": ["https://www.googleapis.com/auth/gmail.modify"],
                    "access_type": "offline",
                    "created_at": "2026-03-26T00:00:00.000Z",
                }))
            })
            .mount(&server)
            .await;

        // --- Connected Accounts: List ---
        let sd = state_dir.clone();
        Mock::given(method("GET"))
            .and(path("/me/v1/connected-accounts/accounts"))
            .respond_with(move |_req: &Request| {
                let accounts = load_accounts(&sd);
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "accounts": accounts,
                }))
            })
            .mount(&server)
            .await;

        // --- Connected Accounts: Delete ---
        let sd = state_dir.clone();
        Mock::given(method("DELETE"))
            .and(path_regex(r"^/me/v1/connected-accounts/accounts/.+$"))
            .respond_with(move |req: &Request| {
                let account_id = req.url.path().rsplit('/').next().unwrap_or("");
                let mut accounts = load_accounts(&sd);
                accounts.retain(|a| {
                    a.get("id").and_then(|v| v.as_str()) != Some(account_id)
                });
                save_accounts(&sd, &accounts);
                ResponseTemplate::new(204)
            })
            .mount(&server)
            .await;

        // --- Logout endpoint (just accept it) ---
        Mock::given(method("GET"))
            .and(path("/v2/logout"))
            .respond_with(ResponseTemplate::new(302))
            .mount(&server)
            .await;

        // --- Connect authorize page (fake browser will hit this) ---
        let sd2 = state_dir.clone();
        Mock::given(method("GET"))
            .and(path("/connected-accounts/connect"))
            .respond_with(move |_req: &Request| {
                // Read the connect state and call the redirect_uri
                // Actually, the fake browser handles the callback — this just needs to return 200
                let _ = &sd2;
                ResponseTemplate::new(200).set_body_string("Connect page")
            })
            .mount(&server)
            .await;

        Self {
            server,
            state_dir,
        }
    }

    pub fn uri(&self) -> String {
        self.server.uri()
    }
}

fn accounts_file(state_dir: &Path) -> PathBuf {
    state_dir.join("e2e-remote-accounts.json")
}

fn load_accounts(state_dir: &Path) -> Vec<serde_json::Value> {
    let path = accounts_file(state_dir);
    match std::fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn save_accounts(state_dir: &Path, accounts: &[serde_json::Value]) {
    let path = accounts_file(state_dir);
    let _ = std::fs::write(&path, serde_json::to_string_pretty(accounts).unwrap());
}

fn base64_url_encode(input: &str) -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    URL_SAFE_NO_PAD.encode(input.as_bytes())
}
