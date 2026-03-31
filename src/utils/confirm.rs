use colored::Colorize;
use std::io::{self, BufRead, IsTerminal, Write};

use crate::utils::error::AppError;

/// Check whether a destructive action is confirmed.
///
/// Three-tier logic matching the Node.js `requireConfirmation` pattern:
/// 1. If `--confirm` or `--yes` was passed, skip the prompt.
/// 2. If stdin is not a TTY (non-interactive / piped), error out.
/// 3. Otherwise, prompt the user interactively on stderr.
pub fn require_confirmation(action: &str, confirmed: bool) -> Result<(), AppError> {
    if confirmed {
        return Ok(());
    }

    if !io::stdin().is_terminal() {
        return Err(AppError::InvalidInput {
            message: format!(
                "Destructive action \"{}\" requires --confirm or --yes flag in non-interactive mode.",
                action
            ),
        });
    }

    eprint!("{} — are you sure? (y/N) ", action.bold());
    io::stderr().flush().ok();

    let mut answer = String::new();
    io::stdin().lock().read_line(&mut answer).map_err(|_| AppError::General {
        message: "Failed to read confirmation input.".to_string(),
    })?;

    if answer.trim().eq_ignore_ascii_case("y") {
        Ok(())
    } else {
        eprintln!("{}", "Cancelled.".dimmed());
        std::process::exit(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmed_flag_bypasses_prompt() {
        assert!(require_confirmation("test action", true).is_ok());
    }

    #[test]
    fn non_tty_without_flag_returns_error() {
        // In test context stdin is not a TTY, so this should error
        let result = require_confirmation("test action", false);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("--confirm or --yes"));
    }
}
