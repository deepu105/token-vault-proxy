use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::thread;

use anyhow::{bail, Context, Result};
use colored::Colorize;
use serde_json::json;

use super::login;
use crate::auth::callback_server::{PORT_RANGE_END, PORT_RANGE_START};
use crate::cli::LoginArgs;
use crate::utils::output::output;
use crate::utils::prompt::{clean_domain, is_interactive, prompt_required};

const CALLBACK_PORTS: std::ops::RangeInclusive<u16> = PORT_RANGE_START..=PORT_RANGE_END;

fn is_command_available(cmd: &str) -> bool {
    let check = if cfg!(windows) { "where" } else { "which" };
    Command::new(check)
        .arg(cmd)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn run_inherited(cmd: &str, args: &[&str]) -> Result<bool> {
    let status = Command::new(cmd)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("Failed to run: {} {}", cmd, args.join(" ")))?;
    Ok(status.success())
}

/// Run a command with inherited stdin but captured stdout/stderr.
/// Each line of output is tee'd to the real stdout/stderr so the user sees it
/// in real time, while the full output is collected and returned.
fn run_inherited_captured(cmd: &str, args: &[&str]) -> Result<(bool, String)> {
    let mut child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to run: {} {}", cmd, args.join(" ")))?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let stdout_handle = thread::spawn(move || {
        let mut collected = Vec::new();
        let reader = BufReader::new(stdout);
        let mut out = std::io::stdout().lock();
        for line in reader.lines().map_while(Result::ok) {
            let _ = writeln!(out, "{line}");
            collected.push(line);
        }
        collected.join("\n")
    });

    let stderr_handle = thread::spawn(move || {
        let reader = BufReader::new(stderr);
        let mut err = std::io::stderr().lock();
        for line in reader.lines().map_while(Result::ok) {
            let _ = writeln!(err, "{line}");
        }
    });

    let status = child.wait()?;
    let stdout_output = stdout_handle.join().unwrap_or_default();
    let _ = stderr_handle.join();

    Ok((status.success(), stdout_output))
}

/// Strip ANSI escape sequences from a string.
pub(crate) fn strip_ansi(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            // Skip ESC [ ... (final byte is 0x40-0x7E)
            i += 2;
            while i < bytes.len() && !(0x40..=0x7E).contains(&bytes[i]) {
                i += 1;
            }
            if i < bytes.len() {
                i += 1; // skip the final byte
            }
        } else {
            result.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&result).to_string()
}

/// Parse the Client ID from `configure-auth0-token-vault` output.
/// Looks for "Client ID:" followed by a non-whitespace token.
/// Strips ANSI escape codes first since the output may come from a PTY.
pub(crate) fn parse_client_id(output: &str) -> Option<String> {
    let clean = strip_ansi(output);
    // Case-insensitive search for "Client ID:" then grab next token
    let lower = clean.to_lowercase();
    let idx = lower.find("client id:")?;
    let after = &clean[idx + "client id:".len()..];
    let token = after.split_whitespace().next()?;
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

fn run_captured(cmd: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .with_context(|| format!("Failed to run: {} {}", cmd, args.join(" ")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("{} failed: {}", cmd, stderr.trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Parse a single tenant domain from `auth0 tenants list --json` output.
/// Returns `Some(domain)` when exactly one tenant exists, `None` otherwise.
pub(crate) fn parse_single_tenant(json_str: &str) -> Option<String> {
    let tenants: serde_json::Value = serde_json::from_str(json_str).ok()?;
    let arr = tenants.as_array()?;
    if arr.len() == 1 {
        return arr[0]["name"]
            .as_str()
            .or_else(|| arr[0]["domain"].as_str())
            .map(|s| s.to_string());
    }
    None
}

/// Parse the client_secret from `auth0 apps show --reveal-secrets --json` output.
pub(crate) fn parse_app_secret(json_str: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(json_str).ok()?;
    json["client_secret"]
        .as_str()
        .or_else(|| json["clientSecret"].as_str())
        .map(|s| s.to_string())
}

/// Try to auto-detect the Auth0 tenant domain from the auth0 CLI.
fn detect_domain() -> Option<String> {
    let output = run_captured("auth0", &["tenants", "list", "--json"]).ok()?;
    if let Some(domain) = parse_single_tenant(&output) {
        return Some(domain);
    }

    // Multiple or zero tenants — try interactive selection
    let tenants: serde_json::Value = serde_json::from_str(&output).ok()?;
    let arr = tenants.as_array()?;
    if arr.len() > 1 {
        eprintln!("\nMultiple tenants detected:");
        for (i, t) in arr.iter().enumerate() {
            let name = t["name"].as_str().unwrap_or("unknown");
            eprintln!("  {}. {}", i + 1, name);
        }
        if let Ok(choice) = prompt_required("Select tenant number: ") {
            if let Ok(idx) = choice.parse::<usize>() {
                if idx > 0 && idx <= arr.len() {
                    return arr[idx - 1]["name"]
                        .as_str()
                        .or_else(|| arr[idx - 1]["domain"].as_str())
                        .map(|s| s.to_string());
                }
            }
        }
    }

    None
}

/// Retrieve domain and client secret from the auth0 CLI, falling back to prompts.
fn get_app_credentials(client_id: &str) -> Result<(String, String)> {
    // Try JSON output first
    if let Ok(output) = run_captured(
        "auth0",
        &["apps", "show", client_id, "--reveal-secrets", "--json"],
    ) {
        if let Some(secret) = parse_app_secret(&output) {
            if let Some(domain) = detect_domain() {
                return Ok((clean_domain(&domain), secret));
            }
        }
    }

    // Fall back: show the app details and prompt
    eprintln!(
        "{} Could not auto-detect credentials. Retrieving app details...\n",
        "!".yellow()
    );
    let _ = run_inherited("auth0", &["apps", "show", client_id, "--reveal-secrets"]);

    eprintln!();
    let domain = prompt_required("Auth0 domain (e.g. your-tenant.eu.auth0.com): ")?;
    let secret = prompt_required("Client secret from above: ")?;
    Ok((clean_domain(&domain), secret))
}

pub async fn run(browser: Option<String>, port: Option<u16>, json_mode: bool) -> Result<()> {
    eprintln!("{}\n", "Auth0 Token Vault Proxy — Setup Wizard".bold());

    if !is_interactive() {
        bail!("The init command requires an interactive terminal.");
    }

    // Check prerequisites
    if !is_command_available("auth0") {
        eprintln!(
            "{} The Auth0 CLI is required but not installed.\n",
            "!".yellow()
        );
        if cfg!(target_os = "macos") {
            eprintln!("  Install via Homebrew:");
            eprintln!("    brew tap auth0/auth0-cli && brew install auth0");
        } else if cfg!(windows) {
            eprintln!("  Install via Scoop:");
            eprintln!("    scoop bucket add auth0 https://github.com/auth0/scoop-auth0-cli");
            eprintln!("    scoop install auth0-cli");
        } else {
            eprintln!("  Install via curl:");
            eprintln!("    curl -sSfL https://raw.githubusercontent.com/auth0/auth0-cli/main/install.sh | sh");
        }
        eprintln!();
        bail!("auth0 CLI not found. Install it and run `tv-proxy init` again.");
    }

    if !is_command_available("npx") {
        eprintln!("{} npx is required but not installed.\n", "!".yellow());
        eprintln!("  Install Node.js: https://nodejs.org/");
        bail!("npx not found. Install Node.js and run `tv-proxy init` again.");
    }

    // Step 1: Configure Token Vault
    eprintln!("{}", "Step 1: Configure Auth0 Token Vault".bold());
    eprintln!("The configuration wizard will guide you through setting up Auth0");
    eprintln!("Token Vault for your tenant.\n");

    let callbacks = CALLBACK_PORTS
        .map(|p| format!("http://127.0.0.1:{p}/callback"))
        .collect::<Vec<_>>()
        .join(",");
    let logout_urls = CALLBACK_PORTS
        .map(|p| format!("http://127.0.0.1:{p}"))
        .collect::<Vec<_>>()
        .join(",");

    let callback_arg = format!("--callback-urls={callbacks}");
    let logout_arg = format!("--logout-urls={logout_urls}");

    let (ok, config_output) = run_inherited_captured(
        "npx",
        &[
            "configure-auth0-token-vault",
            "--",
            "--flavor=refresh_token_exchange",
            &callback_arg,
            &logout_arg,
        ],
    )?;

    if !ok {
        bail!("Token Vault configuration failed. Fix the issue and run `tv-proxy init` again.");
    }

    // Auto-detect Client ID from wizard output, fall back to prompt
    eprintln!();
    let client_id = if let Some(id) = parse_client_id(&config_output) {
        eprintln!("{} Detected Client ID: {id}", "✓".green());
        id
    } else {
        prompt_required("Enter the Client ID from the output above: ")?
    };

    // Step 2: Retrieve client secret and domain
    eprintln!("\n{}", "Step 2: Retrieve credentials".bold());

    let (domain, client_secret) = get_app_credentials(&client_id)?;

    eprintln!("{} Credentials retrieved.", "✓".green());
    eprintln!("  Domain:    {}", domain);
    eprintln!("  Client ID: {}\n", client_id);

    // Step 3: Login
    eprintln!("{}", "Step 3: Authenticate".bold());

    let login_args = LoginArgs {
        domain: Some(domain),
        client_id: Some(client_id),
        client_secret: Some(client_secret),
        audience: None,
        reconfigure: false,
    };

    login::run(login_args, browser, port, json_mode).await?;

    // Next steps
    eprintln!("\n{}\n", "🎉 Setup complete!".bold());
    eprintln!("{}", "Next steps:".bold());
    eprintln!("  {} Connect a provider:", "1.".dimmed());
    eprintln!("     tv-proxy connect gmail");
    eprintln!("     tv-proxy connect github");
    eprintln!("     tv-proxy connect slack");
    eprintln!("  {} Make authenticated API calls:", "2.".dimmed());
    eprintln!("     tv-proxy fetch gmail https://gmail.googleapis.com/gmail/v1/users/me/messages");
    eprintln!("  {} Check status:", "3.".dimmed());
    eprintln!("     tv-proxy status");

    output(json!({"status": "setup_complete"}), "", json_mode);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parse_single_tenant ---

    #[test]
    fn parse_single_tenant_one_entry() {
        let json = r#"[{"name": "my-tenant.auth0.com"}]"#;
        assert_eq!(
            parse_single_tenant(json),
            Some("my-tenant.auth0.com".to_string())
        );
    }

    #[test]
    fn parse_single_tenant_domain_field() {
        let json = r#"[{"domain": "my-tenant.eu.auth0.com"}]"#;
        assert_eq!(
            parse_single_tenant(json),
            Some("my-tenant.eu.auth0.com".to_string())
        );
    }

    #[test]
    fn parse_single_tenant_name_preferred_over_domain() {
        let json = r#"[{"name": "a.auth0.com", "domain": "b.auth0.com"}]"#;
        assert_eq!(parse_single_tenant(json), Some("a.auth0.com".to_string()));
    }

    #[test]
    fn parse_single_tenant_empty_array() {
        assert_eq!(parse_single_tenant("[]"), None);
    }

    #[test]
    fn parse_single_tenant_multiple_returns_none() {
        let json = r#"[{"name": "a.auth0.com"}, {"name": "b.auth0.com"}]"#;
        assert_eq!(parse_single_tenant(json), None);
    }

    #[test]
    fn parse_single_tenant_invalid_json() {
        assert_eq!(parse_single_tenant("not json"), None);
    }

    #[test]
    fn parse_single_tenant_not_array() {
        assert_eq!(parse_single_tenant(r#"{"name": "x"}"#), None);
    }

    // --- parse_app_secret ---

    #[test]
    fn parse_app_secret_standard_field() {
        let json = r#"{"client_id": "abc", "client_secret": "super-secret"}"#;
        assert_eq!(parse_app_secret(json), Some("super-secret".to_string()));
    }

    #[test]
    fn parse_app_secret_camel_case() {
        let json = r#"{"clientId": "abc", "clientSecret": "camel-secret"}"#;
        assert_eq!(parse_app_secret(json), Some("camel-secret".to_string()));
    }

    #[test]
    fn parse_app_secret_missing() {
        let json = r#"{"client_id": "abc", "name": "My App"}"#;
        assert_eq!(parse_app_secret(json), None);
    }

    #[test]
    fn parse_app_secret_invalid_json() {
        assert_eq!(parse_app_secret("not json"), None);
    }

    #[test]
    fn parse_app_secret_null_value() {
        let json = r#"{"client_secret": null}"#;
        assert_eq!(parse_app_secret(json), None);
    }

    // --- strip_ansi ---

    #[test]
    fn strip_ansi_removes_color_codes() {
        assert_eq!(strip_ansi("\x1b[32m✓\x1b[0m Done"), "✓ Done");
    }

    #[test]
    fn strip_ansi_noop_for_plain_text() {
        assert_eq!(strip_ansi("plain text"), "plain text");
    }

    #[test]
    fn strip_ansi_removes_bold_and_reset() {
        assert_eq!(strip_ansi("\x1b[1mBold\x1b[0m"), "Bold");
    }

    // --- parse_client_id ---

    #[test]
    fn parse_client_id_from_output() {
        let output = "Your application Client ID: abc123def456\nDone.";
        assert_eq!(parse_client_id(output), Some("abc123def456".to_string()));
    }

    #[test]
    fn parse_client_id_with_ansi() {
        let output = "\x1b[1mClient ID:\x1b[0m abc123def456";
        assert_eq!(parse_client_id(output), Some("abc123def456".to_string()));
    }

    #[test]
    fn parse_client_id_case_insensitive() {
        let output = "client id: myClientId789";
        assert_eq!(parse_client_id(output), Some("myClientId789".to_string()));
    }

    #[test]
    fn parse_client_id_not_found() {
        let output = "Setup complete. No ID here.";
        assert_eq!(parse_client_id(output), None);
    }

    #[test]
    fn parse_client_id_empty_after_label() {
        let output = "Client ID:";
        assert_eq!(parse_client_id(output), None);
    }
}
