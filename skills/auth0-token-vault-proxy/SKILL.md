---
name: auth0-token-vault-proxy
description: >
  Make authenticated HTTP requests to third-party APIs (Gmail, Slack, GitHub,
  Google Calendar, and more) on behalf of authenticated users via Auth0 Token
  Vault. Use when the user wants to make authenticated API calls to third-party
  services, connect or disconnect OAuth providers, or check their authentication
  and connection status. Wraps the tv-proxy CLI as a generic authenticated fetch
  proxy.
compatibility: Requires tv-proxy binary installed (cargo install or download from GitHub releases)
license: MIT
allowed-tools: Bash(tv-proxy *)
metadata:
  author: auth0
  version: '0.1'
  openclaw:
    emoji: "\U0001F510"
    requires:
      bins:
        - tv-proxy
    os:
      - darwin
      - linux
    install:
      - id: cargo
        kind: cargo
        package: 'token-vault-proxy'
        bins: [tv-proxy]
        label: 'Install tv-proxy (cargo)'
---

# Auth0 Token Vault Proxy

Use the `tv-proxy` command-line tool to make authenticated API requests to
third-party services on behalf of authenticated users via Auth0 Token Vault.

## Current status

- Auth status: !`tv-proxy --json status 2>/dev/null || echo '{"error":{"code":"not_configured","message":"tv-proxy not configured or not logged in"}}'`

## First-time setup

If `tv-proxy --json status` returns a `not_configured` error, guide the user through setup:

1. **Install Auth0 CLI** (if not already installed):

   ```bash
   brew tap auth0/auth0-cli && brew install auth0
   ```

2. **Run the Token Vault setup wizard** (interactive — requires human):

   ```bash
   npx configure-auth0-token-vault
   ```

   The wizard handles Auth0 CLI login automatically. When prompted:
   - Select **Create a new application** (or use an existing one)
   - Select **Regular Web Application** for the app type
   - Select **Refresh Token Exchange** for the Token Vault configuration

   Note the **Client ID** from the output.

3. **Configure callback URLs** using the Auth0 CLI (replace `<APP_ID>` with the Client ID):

   ```bash
   auth0 apps update <APP_ID> \
     --callbacks "http://127.0.0.1:18484/callback,http://127.0.0.1:18485/callback,http://127.0.0.1:18486/callback,http://127.0.0.1:18487/callback,http://127.0.0.1:18488/callback,http://127.0.0.1:18489/callback" \
     --logout-urls "http://127.0.0.1:18484,http://127.0.0.1:18485,http://127.0.0.1:18486,http://127.0.0.1:18487,http://127.0.0.1:18488,http://127.0.0.1:18489"
   ```

4. **Get the client secret** (needed during `tv-proxy login`):

   ```bash
   auth0 apps show <APP_ID> --reveal-secrets
   ```

5. **Log in with tv-proxy:**
   ```bash
   tv-proxy login
   ```

All setup steps require human interaction. Do not attempt to run them autonomously.

## When to use this skill

- The user wants to make an authenticated API call to a third-party service
- The user wants to connect or disconnect a third-party OAuth provider (Google, Slack, GitHub)
- The user asks about their authentication or connection status
- The user wants to interact with Gmail, Google Calendar, Slack, or GitHub APIs via authenticated fetch

## Key patterns

### Always use --json mode

All commands must use `--json` for structured output:

```bash
tv-proxy --json <command>
```

### Exit codes and recovery

| Code | Meaning             | Recovery action                                      |
| ---- | ------------------- | ---------------------------------------------------- |
| 0    | Success             | Parse JSON output                                    |
| 1    | General error       | Report error to user                                 |
| 2    | Invalid input       | Check command syntax and required flags              |
| 3    | Auth required       | Tell the user to run `tv-proxy login`                |
| 4    | Connection required | Tell the user to run `tv-proxy connect <provider>`   |
| 5    | Service error       | Retry or report upstream API failure                 |
| 6    | Network error       | Check connectivity, retry                            |

**Important:** Exit codes 3 and 4 require human intervention — `login` and `connect` open a browser for OAuth. Do not attempt to run these commands autonomously; instead, tell the user what to run.

Auth and connect/logout callback servers default to trying ports `18484-18489`. If that range is blocked, pass the global `--port <number>` flag or set `TV_PROXY_PORT` to force a specific port (that port must be allowed in Auth0 app callback settings).

## Available commands

### Authentication & setup

- `tv-proxy login` — authenticate via browser (human-in-the-loop)
- `tv-proxy login --connection google-oauth2` — use a specific Auth0 connection
- `tv-proxy logout` — clear stored credentials
- `tv-proxy logout --local` — clear only local credentials
- `tv-proxy status` — show current user and connected providers
- `tv-proxy connect <provider>` — connect a provider via browser (human-in-the-loop)
- `tv-proxy connect <provider> --allowed-domains <domains>` — connect with extra allowed domains for `fetch`
- `tv-proxy disconnect <provider>` — disconnect a provider (local token only by default)
- `tv-proxy disconnect <provider> --remote` — disconnect and remove the server-side connection
- `tv-proxy connections` — list connected providers
- `tv-proxy init` — interactive guided setup wizard

### API passthrough (fetch)

- `tv-proxy fetch <service> <url>` — make an authenticated HTTP request to an allowed domain
- `tv-proxy fetch <service> <url> -X POST -d '{"key":"value"}'` — POST with inline body
- `tv-proxy fetch <service> <url> -X POST --data-file ./body.json` — POST with body from file
- `tv-proxy fetch <service> <url> -H "Accept: text/plain"` — add custom headers

Each provider has default allowed domains built in:

| Provider   | Default allowed domains    |
| ---------- | -------------------------- |
| `gmail`    | `*.googleapis.com`         |
| `calendar` | `*.googleapis.com`         |
| `github`   | `api.github.com`           |
| `slack`    | `slack.com`, `*.slack.com` |

Additional domains can be added via `--allowed-domains` on `connect`. Only HTTPS URLs are allowed.

See [references/commands.md](references/commands.md) for full command reference with flags and JSON output examples.
