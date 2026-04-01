use anyhow::Result;

use crate::cli::Commands;

pub mod connect;
pub mod connections;
pub mod disconnect;
pub mod fetch;
pub mod init;
pub mod login;
pub mod logout;
pub mod status;

pub async fn dispatch(
    command: Commands,
    browser: Option<String>,
    port: Option<u16>,
    json_mode: bool,
    confirmed: bool,
) -> Result<()> {
    match command {
        Commands::Login(args) => login::run(args, browser, port, json_mode).await,
        Commands::Logout(args) => logout::run(args, browser, port, json_mode, confirmed).await,
        Commands::Status => status::run(json_mode).await,
        Commands::Connect(args) => connect::run(args, browser, port, json_mode).await,
        Commands::Disconnect(args) => disconnect::run(args, json_mode, confirmed).await,
        Commands::Connections => connections::run(json_mode).await,
        Commands::Fetch(args) => fetch::run(args, json_mode).await,
        Commands::Init => init::run(browser, port, json_mode).await,
    }
}
