# Auth0 Token Vault Proxy - Code Review Report

**Date:** 2026-03-31
**Scope:** Full project review of `token-vault-proxy/` (~2,200 lines of Rust)
**Intent:** Rust CLI that authenticates via Auth0 PKCE, connects third-party OAuth providers, caches tokens locally, and proxies authenticated HTTP requests.
**Reviewers:** correctness, security, maintainability

---

## Findings

### P1 - Should Fix

**1. Dead CLI flags: `--confirm` and `--yes` are never consumed**
`src/cli.rs:19-39` | maintainability | confidence: 0.97

`confirm`, `yes`, and `is_confirmed()` are defined on `Cli` but never forwarded to `dispatch()` or any command handler. They appear in `--help` output suggesting functionality that doesn't exist.

**2. Four unused Cargo dependencies: `dialoguer`, `openidconnect`, `jsonwebtoken`, `colored`**
`Cargo.toml:17-22` | maintainability | confidence: 0.96

None of these are imported anywhere under `src/`. They increase compile time, binary size, and audit surface. Likely leftovers from early development before manual implementations replaced them.

---

### P2 - Fix If Straightforward

**3. Client secret echoed in plaintext in `init` wizard prompt**
`src/commands/init.rs:51` | security | confidence: 0.95

When re-running `tv-proxy init`, the existing `client_secret` is passed as the default value to `prompt()`, which prints it as `Client Secret [s3cr3t_actual_value]:` to stderr. Should use a masked placeholder like `[****]`.

**4. HTML injection possible in `html_page()` (XSS-ready sink)**
`src/auth/callback_server.rs:115` | security | confidence: 0.82

`html_page(title, message)` interpolates parameters directly into HTML without escaping. Currently safe (only hardcoded literals), but the function is `pub` - any future caller passing user-influenced data would create reflected XSS. Should HTML-escape `<`, `>`, `&`, `"`.

**5. `reqwest::Client` constructed fresh on every HTTP call (6 sites)**
`src/auth/connected_accounts.rs:10`, `oidc_config.rs:26`, `pkce_flow.rs:120`, `token_exchange.rs:43`, `token_refresh.rs:27`, `fetch.rs:153` | maintainability | confidence: 0.92

`reqwest::Client` holds a connection pool internally - constructing it per-call discards reuse. A single shared constructor or `LazyLock<reqwest::Client>` would centralize timeout config and enable connection reuse.

**6. Timestamp helper duplicated 8 times across codebase**
`src/auth/pkce_flow.rs:150`, `token_refresh.rs:55`, `status.rs:47`, `connections.rs:13`, `connect.rs:107`, `fetch.rs:134`, `credential_store.rs:162` | maintainability | confidence: 0.94

The same 3-line `SystemTime::now().duration_since(UNIX_EPOCH).as_millis() as i64` pattern is copy-pasted everywhere. A single `pub fn now_ms() -> i64` in `utils` would eliminate all copies.

**7. Test-only `merge_config_pure` duplicates production `merge_config` logic verbatim**
`src/utils/config.rs:290-328` | maintainability | confidence: 0.91

`merge_config_pure` is a line-for-line copy of `merge_config`. If the merge logic changes, both copies must stay in sync. The fix is to make the pure version the core implementation and have `merge_config` delegate to it.

**8. Identity `.map(|t| t)` no-op**
`src/commands/connections.rs:33` | maintainability | confidence: 0.99

`store.get_auth0_tokens().map(|t| t)` - the `.map(|t| t)` does nothing and obscures intent. Remove it.

**9. `OidcEndpoints` and `OidcConfiguration` are identical structs**
`src/auth/oidc_config.rs:7-18` | maintainability | confidence: 0.93

Both have exactly the same three `String` fields. The `discover` function maps field-by-field between them. Collapse to a single struct with `#[derive(Deserialize)]`.

**10. `ConnectCompleteResponse` duplicates `ConnectedAccount` field-for-field**
`src/auth/connected_accounts.rs:36-40` | maintainability | confidence: 0.93

Same pattern - identical struct used only to deserialize and then manually copy into the public type. Just deserialize into `ConnectedAccount` directly.

**11. `AppError::General` variant is defined but never constructed**
`src/utils/error.rs:8` | maintainability | confidence: 0.95

Generic errors in `main.rs:39` flow through un-downcasted `anyhow::Error` with `EXIT_GENERAL` directly. The `General` variant is dead code.

**12. No credential file permissions on Windows**
`src/store/file_backend.rs:204-208` | security | confidence: 0.78

`set_file_permissions` is a no-op on non-Unix. The credential file containing client secrets and tokens gets default ACLs on Windows.

**13. Tokens stored as plain `String` - no protection against accidental logging**
`src/store/types.rs` | maintainability/security | confidence: 0.75

`Auth0Tokens`, `ConnectionToken`, and `StoredConfig` all derive `Debug` with sensitive fields as plain `String`. A stray `{:?}` or `tracing::debug!(?tokens)` dumps secrets to logs. Consider `secrecy::SecretString` or a custom redacting `Debug` impl.

**14. Duplicated HTTP error-check-and-bail pattern (8 sites)**
`src/auth/connected_accounts.rs`, `oidc_config.rs`, `pkce_flow.rs`, `token_refresh.rs` | maintainability | confidence: 0.85

The same 5-line `if !response.status().is_success() { bail!(...) }` pattern repeats 8 times across auth modules. A shared helper would consolidate error formatting.

---

### P3 - User's Discretion

**15. No timeout on callback server wait - indefinite hang possible**
`src/auth/callback_server.rs:103` | security | confidence: 0.90

`CallbackServer::wait()` has no timeout. If the browser never redirects back, the CLI hangs forever. For agent integrations, this is a blocking issue. A `tokio::time::timeout(Duration::from_secs(120), ...)` would bound it.

**16. TOCTOU: temp file briefly world-readable before chmod**
`src/store/file_backend.rs:84` | security | confidence: 0.70

`fs::write` creates the temp file with umask-derived permissions (typically 0644), then `set_file_permissions` restricts to 0600. Between those two calls, the file is readable. Mitigated by the 0700 parent directory, but using `OpenOptionsExt::mode(0o600)` at creation would eliminate the window.

**17. Directory permissions only set on first creation**
`src/store/file_backend.rs:105` | security | confidence: 0.80

`ensure_dir` only sets 0700 when the directory doesn't exist. If it was pre-created with 0755 (e.g., manually), subsequent runs don't correct it.

**18. JWT decoded without signature verification in `status`**
`src/commands/status.rs:98` | security | confidence: 0.75

`decode_id_token` parses JWT claims without signature verification. Only used for display, but a tampered credential file could show forged identity info in `--json` output consumed by agents.

**19. `html_page` is `pub` but only used within its module**
`src/auth/callback_server.rs:115` | maintainability | confidence: 0.90

Reduce to `fn` (private) or `pub(crate)`.

**20. `TokenResponse::token_type` uses `#[allow(dead_code)]`**
`src/auth/pkce_flow.rs:49` | maintainability | confidence: 0.88

Serde ignores unknown fields by default - just remove the field instead of suppressing the warning.

---

## Coverage

**Residual risks:**
- `TV_PROXY_AUTH0_BASE_URL` can redirect all Auth0 API calls to an arbitrary server (intended for testing, no runtime warning)
- `TV_PROXY_ALLOW_HTTP` bypasses HTTPS enforcement with no warning emitted
- No file locking in FileBackend - concurrent `tv-proxy` processes can race on load/persist
- OIDC discovery is never cached - every operation makes a fresh HTTP call

**Testing gaps:**
- No unit tests for any auth module (`pkce_flow`, `token_exchange`, `token_refresh`, `connected_accounts`, `oidc_config`)
- No unit test for `decode_id_token` (base64 padding recovery, malformed JWTs)
- No test for the stored+registry domain merge logic in `fetch.rs`
- No Windows-specific test for credential file access control
- No test for callback server timeout/concurrent requests

---

## Verdict

**Ready with fixes.** No P0 blockers. The P1 items (dead flags, unused deps) are quick wins. The P2 security items (secret in prompt, XSS sink) should be addressed before any release. The maintainability P2s (dedup, shared client) are recommended for code health.

Suggested fix order:
1. Remove unused Cargo deps and dead CLI flags (P1)
2. Mask client secret in init prompt (P2 security)
3. HTML-escape `html_page` parameters (P2 security)
4. Extract shared `now_ms()` and `http_client()` helpers (P2 dedup)
5. Collapse duplicate structs, remove dead variant (P2 cleanup)
