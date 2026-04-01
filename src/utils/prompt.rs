use std::io::{self, BufRead, IsTerminal, Write};

use anyhow::{bail, Result};
use colored::Colorize;

use super::config::merge_config_with_flags;
use crate::store::types::StoredConfig;

/// Strip protocol prefix and trailing slashes from a domain string.
///
/// No regex validation is applied here. The result is always used in
/// `format!("https://{domain}")` constructors downstream, which reject
/// malformed domains, providing defense-in-depth against injection.
pub fn clean_domain(domain: &str) -> String {
    let d = domain.strip_prefix("https://").unwrap_or(domain);
    let d = d.strip_prefix("http://").unwrap_or(d);
    d.trim_end_matches('/').to_string()
}

/// Prompt for a required value on stderr, repeating until non-empty.
pub fn prompt_required(label: &str) -> Result<String> {
    loop {
        eprint!("{}", label);
        io::stderr().flush()?;
        let mut line = String::new();
        io::stdin().lock().read_line(&mut line)?;
        let trimmed = line.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
        eprintln!("  {}", "This field is required.".dimmed());
    }
}

/// Prompt for an optional value on stderr.
pub fn prompt_optional(label: &str) -> Result<String> {
    eprint!("{}", label);
    io::stderr().flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim().to_string())
}

/// Resolve Auth0 config by merging flags → env vars → stored values, then
/// interactively prompting for any fields still missing.
pub fn resolve_config_with_prompts(
    flag_domain: Option<&str>,
    flag_client_id: Option<&str>,
    flag_client_secret: Option<&str>,
    flag_audience: Option<&str>,
    stored: Option<&StoredConfig>,
) -> Result<StoredConfig> {
    let merged = merge_config_with_flags(
        flag_domain,
        flag_client_id,
        flag_client_secret,
        flag_audience,
        stored,
    );

    // All resolved — no prompts needed
    if merged.missing.is_empty() {
        return Ok(StoredConfig {
            domain: clean_domain(&merged.domain.unwrap()),
            client_id: merged.client_id.unwrap(),
            client_secret: merged.client_secret.unwrap(),
            audience: merged.audience,
        });
    }

    // Need to prompt — check TTY
    if !io::stdin().is_terminal() {
        bail!(
            "Cannot prompt for configuration in non-interactive mode. Set {} environment variable{}.",
            merged.missing.join(", "),
            if merged.missing.len() > 1 { "s" } else { "" }
        );
    }

    eprintln!("\n{}\n", "Auth0 configuration required.".bold());

    let domain = match merged.domain {
        Some(d) => d,
        None => prompt_required("Auth0 domain (e.g. your-tenant.eu.auth0.com): ")?,
    };
    let client_id = match merged.client_id {
        Some(c) => c,
        None => prompt_required("Client ID: ")?,
    };
    let client_secret = match merged.client_secret {
        Some(s) => s,
        None => prompt_required("Client secret: ")?,
    };
    let audience = match merged.audience {
        Some(a) => Some(a),
        None => {
            let a = prompt_optional("Audience (optional, press Enter to skip): ")?;
            if a.is_empty() {
                None
            } else {
                Some(a)
            }
        }
    };

    Ok(StoredConfig {
        domain: clean_domain(&domain),
        client_id,
        client_secret,
        audience,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_domain_strips_protocol() {
        assert_eq!(clean_domain("https://foo.auth0.com"), "foo.auth0.com");
        assert_eq!(clean_domain("http://foo.auth0.com"), "foo.auth0.com");
    }

    #[test]
    fn clean_domain_strips_trailing_slashes() {
        assert_eq!(clean_domain("foo.auth0.com///"), "foo.auth0.com");
    }

    #[test]
    fn clean_domain_noop_for_clean_input() {
        assert_eq!(clean_domain("foo.auth0.com"), "foo.auth0.com");
    }

    #[test]
    fn clean_domain_strips_both() {
        assert_eq!(
            clean_domain("https://foo.eu.auth0.com/"),
            "foo.eu.auth0.com"
        );
    }

    #[test]
    fn resolve_config_all_from_flags() {
        let config = resolve_config_with_prompts(
            Some("https://test.auth0.com/"),
            Some("my-id"),
            Some("my-secret"),
            Some("https://api"),
            None,
        )
        .unwrap();

        assert_eq!(config.domain, "test.auth0.com");
        assert_eq!(config.client_id, "my-id");
        assert_eq!(config.client_secret, "my-secret");
        assert_eq!(config.audience.as_deref(), Some("https://api"));
    }

    #[test]
    fn resolve_config_from_stored() {
        let stored = StoredConfig {
            domain: "stored.auth0.com".to_string(),
            client_id: "stored-id".to_string(),
            client_secret: "stored-secret".to_string(),
            audience: None,
        };
        let config = resolve_config_with_prompts(None, None, None, None, Some(&stored)).unwrap();

        assert_eq!(config.domain, "stored.auth0.com");
        assert_eq!(config.client_id, "stored-id");
        assert_eq!(config.client_secret, "stored-secret");
        assert!(config.audience.is_none());
    }

    #[test]
    fn resolve_config_flags_override_stored() {
        let stored = StoredConfig {
            domain: "stored.auth0.com".to_string(),
            client_id: "stored-id".to_string(),
            client_secret: "stored-secret".to_string(),
            audience: None,
        };
        let config =
            resolve_config_with_prompts(Some("flag.auth0.com"), None, None, None, Some(&stored))
                .unwrap();

        assert_eq!(config.domain, "flag.auth0.com");
        assert_eq!(config.client_id, "stored-id"); // from stored
    }

    #[test]
    fn resolve_config_non_tty_missing_fields_errors() {
        // In CI, stdin is not a TTY — this should error with instructions.
        if io::stdin().is_terminal() {
            eprintln!("skipping: stdin is a TTY");
            return;
        }
        let err = resolve_config_with_prompts(None, None, None, None, None).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Cannot prompt"));
        assert!(msg.contains("AUTH0_DOMAIN"));
    }
}
