pub mod auth;
mod cli;
mod commands;
pub mod registry;
pub mod store;
mod utils;

use clap::Parser;
use cli::Cli;
use utils::error::AppError;
use utils::exit_codes::EXIT_GENERAL;
use utils::output::is_json_mode;

#[tokio::main]
async fn main() {
    // Initialize tracing (debug logging to stderr)
    let env_filter = std::env::var("TV_PROXY_LOG")
        .or_else(|_| std::env::var("RUST_LOG"))
        .unwrap_or_default();
    if !env_filter.is_empty() {
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_writer(std::io::stderr)
            .init();
    }

    let cli = Cli::parse();
    let json_mode = is_json_mode(cli.json);

    match commands::dispatch(cli.command, cli.browser, cli.port, json_mode).await {
        Ok(()) => {}
        Err(err) => {
            // Check if it's an AppError with a specific exit code
            if let Some(app_err) = err.downcast_ref::<AppError>() {
                utils::output::output_error(app_err.error_code(), &app_err.to_string(), json_mode);
                std::process::exit(app_err.exit_code());
            }
            // Generic error
            utils::output::output_error("general_error", &err.to_string(), json_mode);
            std::process::exit(EXIT_GENERAL);
        }
    }
}
