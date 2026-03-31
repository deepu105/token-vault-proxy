---
date: 2026-03-30
topic: token-vault-proxy-rust-cli
---

# Token Vault Proxy (tv-proxy) — Rust CLI

## Problem Frame

AI agents and human developers need to make authenticated API calls to third-party services (Gmail, Slack, GitHub, Google Calendar, etc.) on behalf of users via Auth0 Token Vault. The existing `auth0-tv` Node.js CLI serves this purpose but requires the Node.js runtime, has ~300ms startup latency, and bundles service-specific commands (gmail search, slack post, etc.) that are unnecessary when the CLI is used as a proxy.

`tv-proxy` is a focused Rust CLI that acts as an authenticated HTTP proxy — it handles Auth0 login, provider connection, token exchange, and HTTP passthrough. It does **not** include service-specific commands (no `gmail search`, `slack channels`, etc.). Agents use `fetch` to call any API directly.

## Requirements

### Provider Registry & Resolution

- R1. Maintain a hierarchical registry: provider > service > scopes + allowed domains. Known providers:
  - `google-oauth2` (alias: `google`): services `gmail` (5 Gmail scopes, `*.googleapis.com`) and `calendar` (2 Calendar scopes, `*.googleapis.com`)
  - `github` (alias: `github`): service `github` (no default scopes — fine-grained, `api.github.com`)
  - `sign-in-with-slack` (alias: `slack`): service `slack` (9 Slack scopes, `slack.com`, `*.slack.com`)
- R2. Support friendly aliases for providers: `google` -> `google-oauth2`, `github` -> `github`, `slack` -> `sign-in-with-slack`. Accept either the alias or the actual Auth0 connection name.
- R3. For unknown providers (not in registry), pass provider name and user-supplied scopes directly to Auth0. Let Auth0 handle validation errors. `--allowed-domains` is mandatory for unknown providers — `fetch` will reject all URLs if no domains were configured.
- R4. All provider/service lookups are case-insensitive.

### Commands

- R5. **`fetch <provider-or-service> <url> [options]`** — Authenticated HTTP passthrough. Accepts provider name, provider alias, or service name as the first argument. Resolves to the correct Auth0 connection, exchanges for a token, injects `Authorization: Bearer <token>`, makes the request, returns the response. Options: `-X <method>`, `-H <header>` (repeatable), `-d <body>`, `--data-file <path>`. URL must be HTTPS. Domain must be in allowed list (registry defaults merged with stored per-service settings). If called with a service name, validate scopes against saved scopes for that service. Non-2xx responses still output the body but exit with code 5.
- R6. **`connect <provider> [options]`** — Connect an OAuth provider via Auth0 Connected Accounts API. Takes provider name or alias. Scope behavior: if no `--service`, requests union of all default scopes for all services under the provider plus any `--scopes` provided. If `--service <name>` specified, requests only that service's default scopes plus `--scopes`. Deduplicates scopes before sending. For unknown providers, pass as-is. Supports `--allowed-domains <csv>` to store per-provider domain restrictions. Uses browser-based Connected Accounts flow (same as auth0-tv: My Account API initiate -> browser -> callback -> complete). Immediately validates with a token exchange after connecting.
- R7. **`connections`** — List connected providers. Remote mode (if logged in): fetches from My Account API, cross-references with local token cache. Local fallback (if not logged in or API fails): lists locally cached connections. Shows provider, services, scopes, token status (valid/expired/none), remote flag.
- R8. **`disconnect <provider> [options]`** — Remove a provider connection. Always removes local cached token. With `--remote`, also deletes the server-side connected account via My Account API DELETE. Remote failure is a warning, not a hard error.
- R9. **`login`** — Browser-based PKCE login. Opens local HTTP server on ports 18484-18489, opens browser to Auth0 authorization URL, exchanges code for tokens. Saves access_token, refresh_token, id_token. Supports `--browser`, `--port` options.
- R10. **`logout`** — Clears stored credentials. With `--local`, skips browser logout. Without it, opens Auth0 `/v2/logout` in browser.
- R11. **`status`** — Shows current user info (decoded from ID token), token status, connected providers, storage backend.
- R12. **`init`** — Full guided interactive setup: detect if auth0 CLI is installed, offer to install it (via brew/curl), run `npx configure-auth0-token-vault`, then run `tv-proxy login`. Provide final summary with instructions.

### Output & Compatibility

- R13. Dual-mode output: human-readable (default) and JSON (`--json` flag or `TV_PROXY_OUTPUT=json` env var). Maintain same exit codes as auth0-tv (1=general, 2=invalid input, 3=auth required, 4=authz required, 5=service error, 6=network error).
- R14. Full behavioral compatibility with auth0-tv: same exit codes, same JSON output shapes for equivalent commands, same env vars (`AUTH0_DOMAIN`, `AUTH0_CLIENT_ID`, `AUTH0_CLIENT_SECRET`, `AUTH0_AUDIENCE`).
- R15. JSON errors always go to stdout (for agent parsing): `{ "error": { "code": "...", "message": "...", "details": ... } }`. Human errors go to stderr.
- R16. Support `--confirm` / `--yes` flag for destructive actions in non-interactive (agent) mode. The `fetch` command with write methods (POST/PUT/PATCH/DELETE) should require confirmation in non-interactive mode.

### Auth & Token Management

- R17. PKCE flow with S256 code challenge, OIDC discovery via `/.well-known/openid-configuration`. Local callback server on 127.0.0.1 ports 18484-18489 with 2-minute timeout.
- R18. Token exchange via Auth0 federated connection access token grant (`urn:auth0:params:oauth:grant-type:token-exchange:federated-connection-access-token`). Subject token = stored refresh token.
- R19. Automatic token refresh when expired (2-minute expiry buffer). Refresh token rotation support (keep old refresh token if new one not returned).
- R20. Connected Accounts flow via My Account API: initiate link, browser authorization, complete link, immediate token exchange validation.

### Credential Storage

- R21. Two-tier storage: file backend (`~/.tv-proxy/credentials.json` with 0600 perms) and OS keyring backend (via `keyring` crate). Backend selection via `TV_PROXY_STORAGE` env var (default: `keyring`, fallback to `file`).
- R22. Store: config (domain, clientId, clientSecret, audience), auth0 tokens (access, refresh, id, expiresAt), connection tokens per provider (accessToken, expiresAt, scopes), service settings (allowedDomains).

### Configuration

- R23. Config resolution: env var takes precedence over stored value. Required: `AUTH0_DOMAIN`, `AUTH0_CLIENT_ID`, `AUTH0_CLIENT_SECRET`. Optional: `AUTH0_AUDIENCE`.
- R24. Config dir: `TV_PROXY_CONFIG_DIR` env var or default `~/.tv-proxy/`.
- R25. Debug logging via `RUST_LOG` or `TV_PROXY_DEBUG` env var, output to stderr.

## Success Criteria

- Single static binary with no runtime dependencies
- All commands work identically to auth0-tv equivalents (same exit codes, JSON shapes)
- Startup time under 100ms
- Cross-platform: Linux (x86_64, aarch64), macOS (x86_64, aarch64), Windows (x86_64)
- Agent integration: an AI agent using auth0-tv today can switch to tv-proxy with zero code changes (compatible env vars, exit codes, JSON output)

## Scope Boundaries

- No service-specific commands (no `gmail search`, `slack channels`, etc.) — agents use `fetch` directly
- No interactive config prompts *except* in `init` and `login` (when config is missing)
- No auto-update mechanism in v1
- No shell completion in v1 (can add later via `clap_complete`)
- No Windows ARM64 in v1

## Key Decisions

- **Provider-centric model**: `connect`/`disconnect` operate on providers, not services. Provider registry groups services under providers. `fetch` accepts provider name, alias, or service name.
- **Full auth0-tv compatibility**: Same exit codes, JSON shapes, env vars. Drop-in replacement for the proxy use case.
- **File + Keyring storage**: Support both backends from the start. Keyring via `keyring` crate. File as fallback.
- **Full guided init**: `init` command handles auth0 CLI installation and Token Vault configuration.
- **No service-specific commands**: This is a proxy, not a service client. The `fetch` command is the primary interface for agents.

## Dependencies / Assumptions

- Auth0 Token Vault must be configured on the Auth0 tenant (the `init` command helps with this)
- OS keyring availability varies (secret-service on Linux may not be present on headless servers — file backend is the fallback)
- The auth0 CLI must be installable for `init` to work fully

## Outstanding Questions

### Deferred to Planning

- [Affects R12][Technical] Exact auth0 CLI commands for Token Vault configuration — needs research during planning
- [Affects R21][Technical] Keyring crate API for credential storage — verify cross-platform behavior during implementation
- [Affects R5][Technical] Whether `fetch` should stream large responses or buffer — can decide during implementation based on reqwest capabilities

## Next Steps

-> `/ce:plan` for structured implementation planning
