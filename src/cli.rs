use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "tv-proxy",
    about = "Auth0 Token Vault Proxy — authenticated HTTP proxy for third-party services",
    version,
    long_about = "Authenticate via Auth0, connect third-party services, and make\nauthenticated API requests from the terminal. Designed for both\nhumans and AI agents."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Output results as JSON (for agent consumption)
    #[arg(long, global = true)]
    pub json: bool,

    /// Skip destructive-action confirmation prompts
    #[arg(long, global = true)]
    pub confirm: bool,

    /// Skip destructive-action confirmation prompts (alias for --confirm)
    #[arg(long, global = true)]
    pub yes: bool,

    /// Browser to open for auth flows (e.g. firefox, google-chrome)
    #[arg(long, global = true)]
    pub browser: Option<String>,

    /// Port for the local OAuth callback server (default: auto-select from 18484-18489)
    #[arg(long, global = true)]
    pub port: Option<u16>,
}

impl Cli {
    /// Whether destructive action confirmation is bypassed
    pub fn is_confirmed(&self) -> bool {
        self.confirm || self.yes
    }
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Browser-based PKCE login
    Login(LoginArgs),

    /// Clear stored credentials and optionally logout from Auth0
    Logout(LogoutArgs),

    /// Show current user info, token status, and connected providers
    Status,

    /// Connect an OAuth provider via Auth0 Connected Accounts
    Connect(ConnectArgs),

    /// Remove a provider connection
    Disconnect(DisconnectArgs),

    /// List connected providers
    Connections,

    /// Make an authenticated HTTP request to a third-party API
    Fetch(FetchArgs),

    /// Interactive guided setup wizard
    Init,
}

#[derive(clap::Args, Debug)]
pub struct LoginArgs {
    /// Auth0 connection to use for login
    #[arg(long)]
    pub connection: Option<String>,

    /// Connection-specific scopes
    #[arg(long)]
    pub connection_scope: Option<String>,

    /// API audience
    #[arg(long)]
    pub audience: Option<String>,

    /// Additional scopes
    #[arg(long)]
    pub scope: Option<String>,
}

#[derive(clap::Args, Debug)]
pub struct LogoutArgs {
    /// Skip browser logout, only clear local credentials
    #[arg(long)]
    pub local: bool,
}

#[derive(clap::Args, Debug)]
pub struct ConnectArgs {
    /// Provider name or alias (e.g. "google", "slack", "github")
    pub provider: String,

    /// Connect only a specific service under the provider (e.g. "gmail", "calendar")
    #[arg(long)]
    pub service: Option<String>,

    /// Additional OAuth scopes (comma-separated)
    #[arg(long)]
    pub scopes: Option<String>,

    /// Allowed domains for fetch (comma-separated, e.g. "*.example.com,api.example.com")
    #[arg(long)]
    pub allowed_domains: Option<String>,
}

#[derive(clap::Args, Debug)]
pub struct DisconnectArgs {
    /// Provider name or alias
    pub provider: String,

    /// Also delete the server-side connected account
    #[arg(long)]
    pub remote: bool,
}

#[derive(clap::Args, Debug)]
pub struct FetchArgs {
    /// Provider, alias, or service name (e.g. "google", "gmail", "google-oauth2")
    pub service: String,

    /// URL to fetch (must be HTTPS)
    pub url: String,

    /// HTTP method
    #[arg(short = 'X', long = "method", default_value = "GET")]
    pub method: String,

    /// Additional headers (Key: Value format, repeatable)
    #[arg(short = 'H', long = "header")]
    pub headers: Vec<String>,

    /// Request body
    #[arg(short = 'd', long = "data")]
    pub data: Option<String>,

    /// Read request body from file
    #[arg(long)]
    pub data_file: Option<String>,
}
