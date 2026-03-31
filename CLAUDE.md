# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What This Is

Auth0 Token Vault Proxy (`tv-proxy`) — a Rust CLI that authenticates users via Auth0, connects third-party providers (Google, Slack, GitHub) through Auth0 Token Vault, and makes authenticated API requests to allowed domains on behalf of users. Designed as a generic fetch proxy for both humans and AI agents (via `--json` output).

Unlike the companion Node.js CLI (`auth0-tv`) which has built-in service clients (Gmail, Calendar, Slack, GitHub), `tv-proxy` is a pure proxy: it handles auth and token management, then passes through HTTP requests to third-party APIs.

## Commands

```bash
cargo build                 # Debug build
cargo build --release       # Release build (LTO + strip)
cargo test                  # Run all tests (unit + integration + e2e)
cargo test -- --test-threads=1  # Run tests serially (if needed)
cargo clippy                # Lint with Clippy
cargo fmt                   # Format code
cargo fmt -- --check        # Check formatting without modifying
cargo run -- <args>         # Run CLI in dev mode
cargo run -- --help         # Show help
```

## Architecture

**CLI framework:** Clap (derive). Entry point is `src/main.rs` which parses args and dispatches to command handlers.

**Command pattern:** Each command lives in `src/commands/` and is an async function that takes parsed args, a credential store, and an output mode. Commands use `output::success()` / `output::error()` from `src/utils/output.rs` to support both human-readable and `--json` output modes.

**Auth flow:** Browser-based PKCE login (`src/auth/pkce_flow.rs`) opens a local HTTP server on ports 18484-18489 (via `src/auth/callback_server.rs`) to receive the OAuth callback. Token exchange for third-party services (`src/auth/token_exchange.rs`) uses Auth0's federated connection access token grant type with the stored refresh token.

**Credential storage:** Two-tier facade pattern:

- `CredentialStore` (facade in `src/store/credential_store.rs`) — handles expiry logic (2-min buffer), caching, and delegates to a backend
- `CredentialBackend` trait (`src/store/mod.rs`) — implemented by `KeyringBackend` (OS keychain) and `FileBackend` (JSON in `~/.tv-proxy/`)
- Backend selection: `TV_PROXY_STORAGE` env var, defaults to `keyring`

**Config resolution:** Config is resolved by merging environment variables with stored values. Required fields: `AUTH0_DOMAIN`, `AUTH0_CLIENT_ID`, `AUTH0_CLIENT_SECRET`. Env vars take precedence.

**Provider registry:** `src/registry/` maps provider names/aliases to Auth0 connection types and default allowed domains. E.g., `gmail` → `google-oauth2` provider with `*.googleapis.com` allowed.

**Exit codes:** Defined in `src/utils/exit_codes.rs` — distinct codes for auth required (3), authorization/connection required (4), service errors (5), and network errors (6).

**Domain validation:** `src/commands/fetch.rs` contains `is_domain_allowed()` which validates request URLs against default + user-configured allowed domains (with wildcard support like `*.googleapis.com`).

## Testing

Tests use:
- **Unit tests:** Inline `#[cfg(test)]` modules in source files
- **Integration tests:** `tests/cli_integration.rs` using `assert_cmd` + `predicates` for CLI invocation testing
- **E2E tests:** `tests/e2e_flow.rs` with `wiremock` (mock HTTP server), a test fixture that runs the real `tv-proxy` binary, and a fake browser script (`tests/e2e/fake_browser.sh`) that simulates OAuth flows

Key test env vars:
- `TV_PROXY_PORT=0` — use OS-assigned ephemeral ports (prevents port conflicts in parallel tests)
- `TV_PROXY_ALLOW_HTTP=1` — bypass HTTPS requirement for e2e tests against local wiremock (HTTP-only)
- `TV_PROXY_STORAGE=file` — use file backend in tests (not OS keyring)
- `TV_PROXY_CONFIG_DIR` — point at temp directory for test isolation

## Key Conventions

- All local dependencies use workspace-local paths
- Error handling uses `anyhow::Result` for commands, `thiserror` for typed errors
- Async runtime is Tokio (full features)
- HTTP client is `reqwest` with `rustls-tls` (no OpenSSL dependency)
- Structured logging via `tracing` + `tracing-subscriber` (enabled with `RUST_LOG` env var)
- The `output` module must be used for all command output to maintain dual-mode (human/JSON) support
- File backend sets 0600 permissions on credential files, 0700 on directories

## Agent Skill

An [Agent Skills](https://agentskills.io) manifest is at `skills/auth0-token-vault-proxy/SKILL.md` (with a symlink at `.claude/skills/` for Claude Code). It defines how agents discover and invoke `tv-proxy`, including exit code recovery, the fetch proxy pattern, and the full command reference in `references/commands.md`.
