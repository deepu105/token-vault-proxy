---
title: "feat: Port E2E tests from JS to Rust"
type: feat
status: active
date: 2026-03-30
origin: test/e2e/ (JS source tests)
---

# feat: Port E2E tests from JS to Rust

## Overview

Port the 12 end-to-end test scenarios from `test/e2e/cli-flow.test.ts` (JS) to `rust-port/tests/e2e_flow.rs` (Rust). The JS e2e tests exercise the full CLI binary through subprocess invocation with a mock Auth0 backend, fake browser, and stateful connected accounts — the same approach must be replicated in Rust using `wiremock`, `assert_cmd`, and a Rust fake-browser binary.

## Problem Frame

The Rust port (`tv-proxy`) has 88 passing tests (72 unit + 16 integration) but no end-to-end tests that exercise the full login → connect → fetch → disconnect → logout lifecycle against a mock Auth0 backend. The JS version has 12 comprehensive e2e scenarios; porting them proves the Rust CLI is behaviourally equivalent.

## Requirements Trace

- R1. All 12 JS e2e test scenarios must have Rust equivalents
- R2. Tests must run without network access — all Auth0 and API calls hit a local mock server
- R3. Tests must exercise the real compiled `tv-proxy` binary (subprocess invocation via `assert_cmd`)
- R4. The fake browser must simulate OAuth login and Connected Accounts callback flows
- R5. Mock state (connected accounts) must persist across CLI invocations within a single test
- R6. Tests must verify exit codes, JSON stdout structure, and credential store side-effects
- R7. The `gmail` subcommand tests from JS (search) should be skipped — `tv-proxy` doesn't have service-specific subcommands; the `fetch` command covers API proxying
- R8. The `--confirm` destructive action test from JS should be skipped — `tv-proxy` doesn't have service-specific delete commands

## Scope Boundaries

- **In scope:** All e2e flows that exercise `tv-proxy` commands (login, logout, status, connect, disconnect, connections, fetch)
- **Out of scope:** Gmail/Calendar/GitHub/Slack service-specific subcommands (JS-only feature), `init` command e2e (interactive stdin), TLS certificate generation
- **Adaptation:** The JS `gmail search` test becomes a `fetch gmail <url>` test; the echo endpoint pattern already exists in the JS mocks

## Context & Research

### Relevant Code and Patterns

- `rust-port/tests/cli_integration.rs` — existing 16 integration tests using `assert_cmd` + `predicates` + `tempfile`, sets `TV_PROXY_STORAGE=file` and `TV_PROXY_CONFIG_DIR` for isolation
- `test/e2e/helpers.ts` — JS fixture pattern: temp dir, controlled env, `run()` wrapper, `cleanup()`
- `test/e2e/runtime/register-mocks.mjs` — JS mock server: monkey-patches `globalThis.fetch` to intercept OIDC discovery, token endpoint (3 grant types), Connected Accounts API, echo endpoint
- `test/e2e/runtime/fake-browser.mjs` — JS fake browser: extracts `redirect_uri`/`state` from URL, calls callback server

### Key Technical Challenge: HTTPS URLs

The Rust code constructs Auth0 URLs using `format!("https://{}/...", config.domain)`. For e2e tests, we need these to hit a local `wiremock` server instead. The cleanest approach is to add a `TV_PROXY_AUTH0_BASE_URL` environment variable that overrides the default `https://{domain}` base. This is a minimal, test-only seam:

- When `TV_PROXY_AUTH0_BASE_URL` is set (e.g., `http://127.0.0.1:PORT`), all Auth0 API calls use that base URL instead of `https://{domain}`
- When unset (production), behavior is unchanged — the existing `https://` construction remains the default
- This pattern is common in CLIs that support Auth0 custom domains or staging environments

### Institutional Learnings

No `docs/solutions/` directory exists.

## Key Technical Decisions

- **Base URL override via env var:** Add `TV_PROXY_AUTH0_BASE_URL` env var to override the default `https://{domain}` base for all Auth0 API calls. This is the simplest approach with minimal production code changes — just a helper function that checks the env var and falls back to `https://{domain}`.
- **Wiremock for mock server:** Use `wiremock 0.6` (already in dev-deps) to serve HTTP mock responses. No TLS needed since the base URL override allows `http://`.
- **Fake browser as test helper binary:** Build a small Rust binary (or use a shell script) that extracts `redirect_uri`/`state` from the authorize URL and calls the callback server. The `TV_PROXY_BROWSER` env var already supports custom browser commands.
- **Stateful mock via shared files:** Like the JS version, use a JSON file in the temp directory to persist connected account state across CLI invocations within a test.
- **Sequential test execution:** Tests share patterns that modify state, so they should run sequentially within each test function (mirroring JS's `describe.sequential`). Separate test functions can run in parallel since each gets its own temp dir and wiremock instance.

## Open Questions

### Resolved During Planning

- **How to handle `https://` hardcoding?** → Use `TV_PROXY_AUTH0_BASE_URL` env var override. Only ~5 files need a one-line change to call a helper instead of `format!("https://{}...", domain)`.
- **How to handle OIDC discovery?** → The wiremock server returns the same discovery JSON but with its own `http://127.0.0.1:PORT` URLs for token_endpoint and authorization_endpoint.
- **How to handle the fetch echo endpoint?** → Wiremock can mount an echo endpoint that returns `{ ok: true, method, authorization }`. The `fetch` command's domain allowlist check needs the echo domain added via `--allowed-domains` at connect time.
- **How to handle `reqwest` rejecting HTTP URLs for OIDC/token calls?** → Not an issue; `reqwest` with `rustls-tls` allows HTTP. The HTTPS enforcement is only in the `fetch` command for user-facing URLs, not internal Auth0 API calls.

### Deferred to Implementation

- Exact wiremock mount syntax for stateful connected accounts (read/write file in mount closure)
- Whether the fake browser should be a separate `[[bin]]` target or an inline script via `std::process::Command`

## Implementation Units

- [ ] **Unit 1: Add `auth0_base_url()` helper and wire it through Auth0 API calls**

  **Goal:** Introduce a single function that resolves the Auth0 base URL, checking `TV_PROXY_AUTH0_BASE_URL` env var first, then falling back to `https://{domain}`. Update all ~5 call sites.

  **Requirements:** R2

  **Dependencies:** None

  **Files:**
  - Modify: `rust-port/src/utils/config.rs` (add `auth0_base_url()` helper)
  - Modify: `rust-port/src/auth/oidc_config.rs` (use helper instead of hardcoded `https://`)
  - Modify: `rust-port/src/auth/token_exchange.rs` (use helper)
  - Modify: `rust-port/src/auth/connected_accounts.rs` (use helper in all ~5 URL constructions)
  - Modify: `rust-port/src/commands/logout.rs` (use helper for `/v2/logout` URL)

  **Approach:**
  - Add `pub fn auth0_base_url(domain: &str) -> String` to `utils/config.rs`
  - Check `std::env::var("TV_PROXY_AUTH0_BASE_URL")` — if set, return it; otherwise return `format!("https://{}", domain)`
  - Replace all `format!("https://{}...", config.domain)` or `format!("https://{}...", domain)` patterns with `format!("{}...", auth0_base_url(&config.domain))` or equivalent
  - All existing unit tests must still pass since the env var won't be set

  **Patterns to follow:**
  - Existing `resolve_browser()` and `resolve_callback_port()` env-var-with-fallback pattern in `utils/config.rs`

  **Test scenarios:**
  - Existing 88 tests still pass (env var not set → `https://` default)
  - When `TV_PROXY_AUTH0_BASE_URL=http://127.0.0.1:9999` is set, constructed URLs use that base

  **Verification:**
  - `cargo test` passes with no regressions
  - `grep -r 'format!("https://{}' rust-port/src/` returns only registry scope URLs (not API call sites)

- [ ] **Unit 2: Create wiremock-based mock Auth0 server module**

  **Goal:** Build a reusable test helper that starts a wiremock server with all the Auth0 endpoint mocks needed for e2e tests.

  **Requirements:** R2, R5

  **Dependencies:** Unit 1

  **Files:**
  - Create: `rust-port/tests/e2e/mod.rs`
  - Create: `rust-port/tests/e2e/mock_server.rs`

  **Approach:**
  - Start a `wiremock::MockServer` instance
  - Mount handlers for:
    - `GET /.well-known/openid-configuration` → return discovery JSON with server's own URLs
    - `POST /oauth/token` with `grant_type=authorization_code` → return mock tokens
    - `POST /oauth/token` with `grant_type=refresh_token` + audience containing `/me/` → return My Account token
    - `POST /oauth/token` with `grant_type=refresh_token` (no `/me/` audience) → return refreshed tokens
    - `POST /oauth/token` with federated connection grant type → check stateful connected accounts, return access token or 403
    - `POST /me/v1/connected-accounts/connect` → return auth_session + connect_uri
    - `POST /me/v1/connected-accounts/complete` → add to stateful accounts, return account
    - `GET /me/v1/connected-accounts/accounts` → return stateful accounts list
    - `DELETE /me/v1/connected-accounts/accounts/{id}` → remove from stateful accounts
  - For stateful endpoints: use a shared JSON file in the test's temp directory (pass path via closure or environment), matching the JS pattern
  - Expose `MockAuth0Server` struct with `uri()` method and `accounts_file` path

  **Patterns to follow:**
  - JS `register-mocks.mjs` endpoint mapping and response shapes
  - `wiremock::MockServer::start()` pattern

  **Test scenarios:**
  - Server starts and responds to OIDC discovery
  - Token endpoint returns appropriate responses for each grant type
  - Connected accounts CRUD is stateful across requests

  **Verification:**
  - Module compiles and mock server is usable from test code

- [ ] **Unit 3: Create fake browser helper**

  **Goal:** Build a test utility that simulates the browser OAuth callback, equivalent to `fake-browser.mjs`.

  **Requirements:** R4

  **Dependencies:** Unit 2 (needs mock server URL for connect_uri)

  **Files:**
  - Create: `rust-port/tests/e2e/fake_browser.sh` (shell script)

  **Approach:**
  - Simple shell script that receives a URL argument
  - For login URLs (has `redirect_uri` param): extract `redirect_uri` and `state`, call `curl "$redirect_uri?code=e2e-auth-code&state=$state"`
  - For connect URLs: read `TV_PROXY_E2E_CONNECT_REDIRECT_URI` and `TV_PROXY_E2E_CONNECT_STATE` env vars (set by the mock server's `/connect` endpoint response handler), call `curl "$redirect_uri?connect_code=e2e-connect-code&state=$state"`
  - For logout URLs (has `returnTo` param): extract `returnTo`, call `curl "$returnTo"`
  - Set via `TV_PROXY_BROWSER` env var pointing to this script
  - The JS version uses env vars (`AUTH0_TV_E2E_CONNECT_REDIRECT_URI`, `AUTH0_TV_E2E_CONNECT_STATE`) to pass connect callback details from the mock server to the fake browser — the Rust version must do the same. The mock `/connect` endpoint handler writes these to the shared state file, and the fake browser reads them.

  **Patterns to follow:**
  - JS `fake-browser.mjs` — extract URL params, call callback server

  **Test scenarios:**
  - Script correctly parses login authorize URL and hits callback
  - Script correctly handles connect flow callback
  - Script correctly handles logout returnTo callback

  **Verification:**
  - Script is executable and handles all three URL types

- [ ] **Unit 4: Create e2e test fixture and helper functions**

  **Goal:** Build the Rust equivalent of `setupE2eFixture()` — temp dir, env var setup, CLI runner function, cleanup.

  **Requirements:** R3, R6

  **Dependencies:** Units 1-3

  **Files:**
  - Create: `rust-port/tests/e2e/fixture.rs`

  **Approach:**
  - `E2eFixture` struct with: `temp_dir: TempDir`, `mock_server: MockAuth0Server`, `base_url: String`
  - `setup()` async function: starts mock server, creates temp dir, returns fixture
  - `run(&self, args: &[&str]) -> CliResult` — invokes `Command::cargo_bin("tv-proxy")` with:
    - `TV_PROXY_AUTH0_BASE_URL` → mock server URI
    - `AUTH0_DOMAIN` → `test.auth0.com`
    - `AUTH0_CLIENT_ID` → `test-client-id`
    - `AUTH0_CLIENT_SECRET` → `test-client-secret`
    - `TV_PROXY_STORAGE` → `file`
    - `TV_PROXY_CONFIG_DIR` → temp dir path
    - `TV_PROXY_BROWSER` → path to fake browser script
    - `NO_COLOR` → `1`
  - `CliResult` struct: `stdout: String`, `stderr: String`, `exit_code: i32`
  - `parse_json(result: &CliResult) -> serde_json::Value` helper
  - `login(fixture: &E2eFixture)` and `login_and_connect_gmail(fixture: &E2eFixture)` convenience helpers

  **Patterns to follow:**
  - JS `helpers.ts` fixture pattern
  - Existing `cli_integration.rs` env var setup pattern

  **Test scenarios:**
  - Fixture creates isolated temp environment
  - CLI binary runs with correct env vars
  - Results capture stdout/stderr/exit code

  **Verification:**
  - Fixture compiles and can run a simple `tv-proxy --help` command

- [ ] **Unit 5: Port core e2e test scenarios**

  **Goal:** Implement the 10 applicable e2e test scenarios as Rust integration tests.

  **Requirements:** R1, R6

  **Dependencies:** Unit 4

  **Files:**
  - Create: `rust-port/tests/e2e_flow.rs` (main test file, imports from `e2e/` module)

  **Approach:**

  The 12 JS tests map to Rust as follows:

  | # | JS Test | Rust Port | Notes |
  |---|---------|-----------|-------|
  | 1 | login → status → connect → connections → gmail search → logout | login → status → connect → connections → fetch (echo endpoint) → logout | Replace `gmail search` with `fetch gmail https://echo.test/echo` |
  | 2 | unauthenticated status, connections, logout | Same | Direct port |
  | 3 | requires connected service before gmail commands | fetch without connect → auth error | Adapt to `fetch` command |
  | 4 | re-login with existing session | Same | Direct port |
  | 5 | persists allowed domains and uses them for fetch | Same | Direct port |
  | 6 | rejects fetch to disallowed domains | Same | Direct port |
  | 7 | local-only and remote disconnect | Same | Direct port |
  | 8 | requires login before remote disconnect | Same | Direct port |
  | 9 | invalid service errors for connect/disconnect/fetch | Same | Direct port |
  | 10 | preserves config after local logout | Same | Direct port |
  | 11 | gmail search (service subcommand) | **Skip** | `tv-proxy` has no service subcommands |
  | 12 | `--confirm` for destructive actions | **Skip** | `tv-proxy` has no service-specific destructive commands |

  Also add an echo endpoint to the wiremock mock server for the `fetch` tests.

  Each test function should be `#[tokio::test]`, create its own fixture, and assert JSON output structure + exit codes.

  **Patterns to follow:**
  - JS `cli-flow.test.ts` assertion patterns: `parseJson(result)`, `toMatchObject`, `toEqual`
  - Rust: `serde_json::from_str`, direct field access, `assert_eq!`

  **Test scenarios (all 10):**
  1. Full happy path: login → status (loggedIn: true) → connect gmail → connections (remote: true, tokenStatus: valid) → fetch echo → logout → verify tokens cleared
  2. Unauthenticated: status (loggedIn: false), connections (empty), logout (not_logged_in)
  3. Fetch without connect: exit code for auth/service error, error JSON with appropriate code
  4. Re-login: second login returns `{ status: "logged_in", reauthenticated: true }`
  5. Allowed domains: connect with `--allowed-domains`, fetch to allowed domain succeeds
  6. Disallowed domain: fetch to unlisted domain returns `domain_not_allowed` error
  7. Disconnect flows: local disconnect (removes local token, remote account persists), remote disconnect (removes both)
  8. Remote disconnect without login: exit code 3, `auth_required` error
  9. Invalid service: connect/disconnect/fetch with unknown service → `invalid_input` error
  10. Config preserved after logout: logout `--local`, status still shows domain and clientId

  **Verification:**
  - All 10 e2e tests pass
  - `cargo test` passes including existing 88 tests

## System-Wide Impact

- **Auth0 base URL helper:** Small change to all Auth0 API call sites (`oidc_config.rs`, `token_exchange.rs`, `connected_accounts.rs`, `logout.rs`). No behavior change when env var is unset.
- **Error propagation:** E2e tests will verify that `AppError` variants produce correct exit codes and JSON error structures end-to-end.
- **State lifecycle:** Connected accounts persist via file-based mock state, matching the JS pattern. Each test gets its own temp dir, preventing cross-test contamination.

## Risks & Dependencies

- **Wiremock stateful handlers:** Wiremock's mount API may need custom `Respond` implementations for stateful endpoints (reading/writing shared files). This may require `wiremock::Mock::respond_with()` with a closure-based responder.
- **Fake browser timing:** The callback server must be listening before the fake browser script tries to connect. The existing `CallbackServer::bind()` → `server.wait()` pattern handles this correctly.
- **Connect flow env var passing:** The mock `/connect` endpoint must communicate `redirect_uri` and `state` to the fake browser. Using a shared file (written by mock, read by fake browser) is more reliable than env vars across processes.

## Sources & References

- JS e2e tests: `test/e2e/cli-flow.test.ts`, `test/e2e/helpers.ts`
- JS mock runtime: `test/e2e/runtime/register-mocks.mjs`, `test/e2e/runtime/fake-browser.mjs`
- Existing Rust integration tests: `rust-port/tests/cli_integration.rs`
- Wiremock docs: https://docs.rs/wiremock/0.6
