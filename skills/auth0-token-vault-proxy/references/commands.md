# tv-proxy Command Reference

Full command reference for agent invocation. All examples use `--json` mode.

## Global options

| Flag              | Description                                                          |
| ----------------- | -------------------------------------------------------------------- |
| `--json`          | Output structured JSON (required for agent use)                      |
| `--confirm`       | Skip destructive-action confirmation prompts                         |
| `--yes`           | Alias for `--confirm`                                                |
| `--browser <app>` | Browser for auth flows (e.g. `firefox`, `google-chrome`)             |
| `--port <number>` | Port for the local OAuth callback server (default: auto 18484-18489) |

## Authentication & setup

### login

Authenticate with Auth0 via browser-based PKCE flow. **Requires human interaction** (opens browser).

```bash
tv-proxy login
tv-proxy login --connection google-oauth2   # use a specific Auth0 connection
tv-proxy login --scope "openid profile"     # request additional scopes
tv-proxy --port 18486 login                 # bind callback server to a specific port
```

| Flag                     | Description                          |
| ------------------------ | ------------------------------------ |
| `--connection <name>`    | Auth0 connection to use for login    |
| `--connection-scope <s>` | Connection-specific scopes           |
| `--audience <url>`       | API audience                         |
| `--scope <scopes>`       | Additional scopes                    |

### logout

Clear all stored credentials and optionally logout from Auth0.

```bash
tv-proxy --json logout
tv-proxy --json logout --local   # clear local credentials only
tv-proxy --json --port 18486 logout
```

| Flag      | Description                                                     |
| --------- | --------------------------------------------------------------- |
| `--local` | Only clear local credentials without ending the browser session |

### status

Show current user info, token status, and connected providers.

```bash
tv-proxy --json status
```

Example JSON output:

```json
{
  "loggedIn": true,
  "user": { "email": "user@example.com", "name": "User Name" },
  "connections": ["google-oauth2"]
}
```

### connect

Connect a third-party provider. **Requires human interaction** (opens browser for OAuth).

```bash
tv-proxy connect google
tv-proxy --port 18486 connect google
tv-proxy connect github --allowed-domains "ghcr.io,uploads.github.com"
tv-proxy connect google --service gmail --scopes "https://www.googleapis.com/auth/gmail.readonly"
```

| Flag                       | Description                                                     |
| -------------------------- | --------------------------------------------------------------- |
| `--service <name>`         | Connect a specific service under the provider (e.g. gmail)      |
| `--scopes <list>`          | Additional OAuth scopes (comma-separated)                       |
| `--allowed-domains <list>` | Comma-separated domains allowed for `tv-proxy fetch` (additive) |

Each provider has default allowed domains built in. Use `--allowed-domains` only to add extra domains beyond the defaults.

### disconnect

Remove a provider connection. By default, only removes the locally-cached token.

```bash
tv-proxy --json disconnect google
tv-proxy --json disconnect google --remote
```

| Flag       | Description                                                |
| ---------- | ---------------------------------------------------------- |
| `--remote` | Also remove the server-side connection (Auth0 Token Vault) |

Example JSON output (local only):

```json
{ "status": "disconnected", "provider": "google", "remote": false }
```

### connections

List all connected providers with their status.

```bash
tv-proxy --json connections
```

### init

Interactive guided setup wizard that walks through Auth0 configuration.

```bash
tv-proxy init
```

## API passthrough (fetch)

Make authenticated HTTP requests to allowed domains using a provider's token.

```bash
tv-proxy --json fetch github https://api.github.com/user
tv-proxy --json fetch gmail https://gmail.googleapis.com/gmail/v1/users/me/messages
tv-proxy --json fetch slack https://slack.com/api/conversations.list
tv-proxy --json fetch github https://api.github.com/repos/octocat/Hello-World/issues -X POST -d '{"title":"Bug"}'
tv-proxy --json fetch github https://api.github.com/user -H "Accept: application/vnd.github.v3+json"
tv-proxy --json fetch slack https://slack.com/api/chat.postMessage -X POST --data-file ./payload.json
```

| Flag                  | Description                               |
| --------------------- | ----------------------------------------- |
| `-X <method>`         | HTTP method (default: GET)                |
| `-H <header>`         | Additional header (Key: Value, repeatable)|
| `-d <body>`           | Request body (inline)                     |
| `--data-file <path>`  | Read request body from file               |

Default allowed domains per provider:

| Provider   | Default allowed domains    |
| ---------- | -------------------------- |
| `gmail`    | `*.googleapis.com`         |
| `calendar` | `*.googleapis.com`         |
| `github`   | `api.github.com`           |
| `slack`    | `slack.com`, `*.slack.com` |

Only HTTPS URLs are allowed. Additional domains can be added via `--allowed-domains` on `connect`.

Example JSON output (success):

```json
{
  "status": 200,
  "headers": { "content-type": "application/json" },
  "body": { "login": "octocat", "id": 1 }
}
```

Example JSON output (domain not allowed):

```json
{
  "error": {
    "code": "domain_not_allowed",
    "message": "Domain 'evil.com' is not in the allowed list for github"
  }
}
```
