# tv-proxy — Auth0 Token Vault Proxy

Authenticated HTTP proxy for third-party [services](https://auth0.com/ai/docs/integrations/overview) via [Auth0 Token Vault](https://auth0.com/ai/docs/intro/token-vault). Make API calls to Gmail, Slack, GitHub, and more — from the terminal or an AI agent — with automatic OAuth token management.

Unlike service-specific CLIs like [Auth0 Token Vault CLI](https://github.com/deepu105/auth0-token-vault-cli), `tv-proxy` is a generic authenticated fetch proxy: connect a provider once, then make authenticated requests to any allowed domain using that provider's token.

## Auth0 Tenant Setup

### Prerequisites

- An [Auth0 Account](https://auth0.com/signup?onboard_app=auth_for_aa&ocid=701KZ000000cXXxYAM-aPA4z0000008OZeGAM)
- [Auth0 CLI](https://github.com/auth0/auth0-cli) installed and logged in
- At least one [connection](https://auth0.com/ai/docs/integrations/overview) configured (e.g. [Google](https://auth0.com/ai/docs/integrations/google))

### Install the Auth0 CLI

```bash
# macOS
brew tap auth0/auth0-cli && brew install auth0

# Other platforms — see https://github.com/auth0/auth0-cli
```

### Configure Token Vault

Run the interactive setup wizard. It logs you into Auth0 CLI then creates and configures an Auth0 application with Token Vault, My Account API, MRRT, and client grants — everything that `tv-proxy` needs:

```bash
npx configure-auth0-token-vault
```

1. When asked, **How would you like to configure the application?**, select **Create a new application**. If you already have an application you'd like to use, select **Use an existing application** and follow the prompts to set it up for Token Vault.
2. If asked, **Select application type**, choose **Regular Web Application**.
3. When asked, **Which Token Vault configuration do you need?**, select **Refresh Token Exchange**.

The wizard will:

- Configure the Regular Web Application with the necessary settings for Token Vault
- Enable the Token Vault grant type
- Activate the My Account API with Connected Accounts scopes
- Create the necessary client grants
- Configure Multi-Resource Refresh Token (MRRT) policies
- Enable your social connections on the application

Note the **Client ID** from the output — you'll need it for `tv-proxy login`.

> **Tip:** The wizard is idempotent — safe to re-run if you need to update the configuration.

### Configure callback URLs

After running the wizard, configure your application's callback and logout URLs for `tv-proxy` using the Auth0 CLI. Replace `<APP_ID>` with the Client ID from the previous step:

```bash
auth0 apps update <APP_ID> \
  --callbacks "http://127.0.0.1:18484/callback,http://127.0.0.1:18485/callback,http://127.0.0.1:18486/callback,http://127.0.0.1:18487/callback,http://127.0.0.1:18488/callback,http://127.0.0.1:18489/callback" \
  --logout-urls "http://127.0.0.1:18484,http://127.0.0.1:18485,http://127.0.0.1:18486,http://127.0.0.1:18487,http://127.0.0.1:18488,http://127.0.0.1:18489"
```

If you plan to use a custom `--port`, add that port's URLs as well.

### Get Client Secret

Retrieve your application's client secret (needed during `tv-proxy login`):

```bash
auth0 apps show <APP_ID> --reveal-secrets
```

## Installation

### Quick install (Linux / macOS)

```bash
curl -fsSL https://raw.githubusercontent.com/deepu105/token-vault-proxy/main/deployment/install.sh | bash
```

Options:

```bash
# Install to a custom directory
BIN_DIR=~/.local/bin curl -fsSL https://raw.githubusercontent.com/deepu105/token-vault-proxy/main/deployment/install.sh | bash

# Install a specific version
VERSION=v0.1.0 curl -fsSL https://raw.githubusercontent.com/deepu105/token-vault-proxy/main/deployment/install.sh | bash
```

### Pre-built binaries

Download the latest release for your platform from [GitHub Releases](https://github.com/deepu105/token-vault-proxy/releases):

| Platform      | Binary                  |
| ------------- | ----------------------- |
| Linux x64     | `tv-proxy-linux-x64`   |
| Linux arm64   | `tv-proxy-linux-arm64`  |
| macOS x64     | `tv-proxy-macos-x64`   |
| macOS arm64   | `tv-proxy-macos-arm64`  |

### Build from source

Requires [Rust](https://www.rust-lang.org/tools/install) 1.75+:

```bash
# From crates.io
cargo install token-vault-proxy

# Or from source
cargo install --path .
```

Or build manually:

```bash
cargo build --release
# Binary at ./target/release/tv-proxy
```

## Quick Start

### 1. Login

```bash
tv-proxy login
```

You'll be prompted for your Auth0 domain, client ID, and client secret, then a browser window opens for authentication.

### 2. Connect a provider

```bash
tv-proxy connect google
tv-proxy connect slack
tv-proxy connect github
```

### 3. Make an authenticated API call

```bash
tv-proxy fetch gmail https://gmail.googleapis.com/gmail/v1/users/me/messages
tv-proxy fetch github https://api.github.com/user
tv-proxy fetch slack https://slack.com/api/conversations.list
```

The `Authorization: Bearer <token>` header is injected automatically.

## Commands

### Authentication & Setup

```bash
tv-proxy login                          # Authenticate via browser-based PKCE flow
tv-proxy login --connection google-oauth2  # Use a specific Auth0 connection
tv-proxy status                         # Show current user and connected providers
tv-proxy connect google                 # Connect Google (opens browser)
tv-proxy connect slack                  # Connect Slack
tv-proxy connect github                 # Connect GitHub
tv-proxy connect github --allowed-domains "ghcr.io"  # Add extra allowed domains for fetch
tv-proxy --port 18486 connect google    # Force callback server to a specific port
tv-proxy connections                    # List connected providers
tv-proxy disconnect google              # Disconnect Google (local only)
tv-proxy disconnect google --remote     # Disconnect Google (local + remote)
tv-proxy logout                         # Clear all stored credentials
tv-proxy --port 18486 logout
tv-proxy init                           # Interactive guided setup wizard
```

### API Passthrough (fetch)

Make authenticated HTTP requests to allowed domains using a provider's token. Only HTTPS URLs are permitted. Each provider has default allowed domains built in:

| Provider   | Default allowed domains    |
| ---------- | -------------------------- |
| `gmail`    | `*.googleapis.com`         |
| `calendar` | `*.googleapis.com`         |
| `github`   | `api.github.com`           |
| `slack`    | `slack.com`, `*.slack.com` |

```bash
tv-proxy fetch github https://api.github.com/user
tv-proxy fetch gmail https://gmail.googleapis.com/gmail/v1/users/me/messages
tv-proxy fetch slack https://slack.com/api/conversations.list
tv-proxy fetch github https://api.github.com/repos/octocat/Hello-World/issues -X POST -d '{"title":"Bug"}'
tv-proxy fetch github https://api.github.com/user -H "Accept: application/vnd.github.v3+json"
tv-proxy fetch slack https://slack.com/api/chat.postMessage -X POST --data-file ./payload.json
```

Add extra domains with `--allowed-domains` on `connect`:

```bash
tv-proxy connect github --allowed-domains "ghcr.io,uploads.github.com"
```

### Global Flags

| Flag              | Description                                                          |
| ----------------- | -------------------------------------------------------------------- |
| `--json`          | Output structured JSON (recommended for agents/scripts)              |
| `--confirm`       | Skip destructive-action confirmation prompts                         |
| `--yes`           | Alias for `--confirm`                                                |
| `--browser <app>` | Browser for auth flows (e.g. `firefox`, `google-chrome`)             |
| `--port <number>` | Port for the local OAuth callback server (default: auto 18484-18489) |

### Exit Codes

| Code | Meaning                                                   |
| ---- | --------------------------------------------------------- |
| 0    | Success                                                   |
| 1    | General error                                             |
| 2    | Invalid input / missing required flag                     |
| 3    | Authentication required (run `tv-proxy login`)            |
| 4    | Authorization required (run `tv-proxy connect <provider>`) |
| 5    | Service error (upstream API failure)                      |
| 6    | Network error                                             |

## Configuration

Set environment variables **or** run `tv-proxy login`, which prompts for the required values and persists them in the credential store. Each field is resolved individually: environment variable takes precedence over stored value.

### Environment Variables

| Variable             | Description                                              |
| -------------------- | -------------------------------------------------------- |
| `AUTH0_DOMAIN`       | Auth0 tenant domain                                      |
| `AUTH0_CLIENT_ID`    | Auth0 application client ID                              |
| `AUTH0_CLIENT_SECRET`| Auth0 application client secret                          |
| `TV_PROXY_STORAGE`   | Credential backend: `keyring` (default) or `file`        |
| `TV_PROXY_CONFIG_DIR`| Override config directory (default: `~/.tv-proxy/`)      |
| `TV_PROXY_BROWSER`   | Browser to open for auth flows (e.g. `firefox`)          |
| `TV_PROXY_PORT`      | Port for the local OAuth callback server                 |

## Agent Integration

The CLI is designed as a skill for [AgentSkills-compatible](https://agentskills.io/) AI agents (OpenClaw, Claude Code, etc.).

### Agent Skills

The CLI ships with an [Agent Skills](https://agentskills.io) manifest that enables automatic discovery in supported agent frameworks.

**Claude Code plugin marketplace:** Install the skill directly in Claude Code:

```
/plugin marketplace add deepu105/token-vault-proxy
```

Then browse and install:

```
/plugin install auth0-token-vault-proxy@auth0-token-vault-proxy
```

**ClawHub (OpenClaw skill registry):** Install the skill via [ClawHub](https://clawhub.ai):

```bash
npx clawhub@latest install auth0-token-vault-proxy
```

**Global installation (manual):** For use outside this repo, install `tv-proxy` and copy the skill:

```bash
# Build and install
cargo install --path .

# Claude Code
cp -r skills/auth0-token-vault-proxy ~/.claude/skills/

# OpenClaw
cp -r skills/auth0-token-vault-proxy ~/.openclaw/skills/
```

**In-project discovery (automatic):** When working in this repo, agents discover the skill automatically:

- **OpenClaw:** via `skills/auth0-token-vault-proxy/SKILL.md`
- **Claude Code:** via `.claude/skills/auth0-token-vault-proxy/SKILL.md` (symlink)

## Credential Storage

Credentials are stored in the OS keyring by default with a fallback to `~/.tv-proxy/credentials.json` with restricted file permissions (0600). Token values are never logged or displayed in CLI output.

## Development

```bash
cargo build                 # Debug build
cargo build --release       # Release build
cargo test                  # Run all tests (unit + integration + e2e)
cargo test -- --test-threads=1  # Run tests serially (if port conflicts)
cargo clippy                # Lint
cargo fmt -- --check        # Check formatting
```

### Project Structure

```
src/
├── main.rs              # Entry point
├── cli.rs               # Clap CLI definitions
├── commands/            # Command handlers (login, logout, connect, disconnect, fetch, etc.)
├── auth/                # PKCE auth flow, callback server, token exchange
├── store/               # Credential store (facade + file/keyring backends)
├── registry/            # Provider registry (google, slack, github mappings)
└── utils/               # Output formatting, config, errors
tests/
├── cli_integration.rs   # CLI integration tests (assert_cmd)
├── e2e_flow.rs          # End-to-end tests (wiremock + real binary)
└── e2e/                 # E2e test helpers (fixture, mock server, fake browser)
```

## Release

Releases are tag-driven. Cross-platform binaries are built, published to GitHub Releases, and the crate is published to [crates.io](https://crates.io) when you push a version tag.

```bash
# Bump version in Cargo.toml, then:
git tag v0.1.0
git push origin main --follow-tags
```

The release workflow will:

- Verify the tagged commit is reachable from `main`
- Run `cargo fmt --check`, `cargo clippy`, and `cargo test`
- Publish the crate to crates.io
- Cross-compile for Linux (x64, arm64) and macOS (x64, arm64)
- Upload binaries as GitHub Release assets
- Create the GitHub release notes automatically

> **Note:** Set the `CARGO_REGISTRY_TOKEN` secret in the repository settings for crates.io publishing.

## License

MIT


## TODO

- [ ] Test all commands for edge case
- [ ] Rewrite the init command