use crate::auth::token_exchange::exchange_for_connection_token;
use crate::cli::FetchArgs;
use crate::registry::{get_allowed_domains, resolve_any, Resolution};
use crate::store::credential_store::CredentialStore;
use crate::store::types::ConnectionToken;
use crate::utils::config::require_config;
use crate::utils::error::AppError;
use crate::utils::http::http_client;
use crate::utils::output::output;
use crate::utils::time::now_ms;
use anyhow::Result;

/// Validate that a URL's hostname is in the allowed domains list.
/// Checks exact match and wildcard subdomain matches (e.g. "*.googleapis.com").
fn is_domain_allowed(hostname: &str, allowed_domains: &[String]) -> bool {
    let hostname = hostname.to_lowercase();
    for domain in allowed_domains {
        let d = domain.to_lowercase();
        if d.starts_with("*.") {
            let suffix = &d[1..]; // ".example.com"
            if hostname.ends_with(suffix) && hostname.len() > suffix.len() {
                return true;
            }
        } else if hostname == d {
            return true;
        }
    }
    false
}

pub async fn run(args: FetchArgs, json_mode: bool) -> Result<()> {
    let service_lower = args.service.to_lowercase();

    // Resolve connection from service/provider name
    let resolution = resolve_any(&service_lower);
    let (connection, service_name_for_domains) = match &resolution {
        Resolution::ProviderMatch(provider) => (provider.connection.to_string(), None),
        Resolution::ServiceMatch(provider, service) => {
            (provider.connection.to_string(), Some(service.name))
        }
        Resolution::Unknown(_) => {
            return Err(AppError::InvalidInput {
                message: format!("Unknown service: {}", args.service),
            }
            .into());
        }
    };

    // Parse and validate URL
    let parsed_url = url::Url::parse(&args.url).map_err(|_| AppError::InvalidInput {
        message: format!("Invalid URL: {}", args.url),
    })?;

    if parsed_url.scheme() != "https" && std::env::var("TV_PROXY_ALLOW_HTTP").is_err() {
        return Err(AppError::InvalidInput {
            message: "Only HTTPS URLs are allowed.".to_string(),
        }
        .into());
    }

    let hostname = parsed_url
        .host_str()
        .ok_or_else(|| AppError::InvalidInput {
            message: "URL has no hostname.".to_string(),
        })?;

    // Check allowed domains (stored settings + registry defaults)
    let store = CredentialStore::from_env()?;
    let settings = store.get_service_settings(&service_lower)?;
    let stored_domains: Vec<String> = settings
        .as_ref()
        .map(|s| s.allowed_domains.clone())
        .unwrap_or_default();

    let registry_domains: Vec<String> = get_allowed_domains(&connection, service_name_for_domains)
        .iter()
        .map(|s| s.to_string())
        .collect();

    let allowed_domains: Vec<String> = if stored_domains.is_empty() {
        registry_domains
    } else {
        let mut combined = stored_domains.clone();
        for d in &registry_domains {
            if !combined.contains(d) {
                combined.push(d.clone());
            }
        }
        combined
    };

    if allowed_domains.is_empty() {
        return Err(AppError::InvalidInput {
            message: format!(
                "No allowed domains configured for {}. Run `tv-proxy connect {} --allowed-domains <domains>` to set them.",
                service_lower, service_lower
            ),
        }
        .into());
    }

    if !is_domain_allowed(hostname, &allowed_domains) {
        return Err(AppError::InvalidInput {
            message: format!(
                "Domain \"{}\" is not in the allowed list for {}. Allowed: {}",
                hostname,
                service_lower,
                allowed_domains.join(", ")
            ),
        }
        .into());
    }

    // Get token — try cache first, then exchange
    let stored_config = store.get_config()?;
    let config = require_config(stored_config.as_ref())?;

    let auth0_tokens = store.get_auth0_tokens()?;
    let refresh_token = auth0_tokens
        .as_ref()
        .and_then(|t| t.refresh_token.as_deref())
        .ok_or_else(|| AppError::AuthRequired {
            message: "Not logged in. Run `tv-proxy login` first.".to_string(),
        })?;

    // Try cached connection token first
    let token = match store.get_connection_token(&connection, &[])? {
        Some(t) => t,
        None => {
            // Exchange for a new token
            let exchange_result =
                exchange_for_connection_token(&config, refresh_token, &connection).await?;

            let now = now_ms();

            let _ = store.save_connection_token(
                &connection,
                &ConnectionToken {
                    access_token: exchange_result.access_token.clone(),
                    expires_at: now + exchange_result.expires_in * 1000,
                    scopes: exchange_result.scopes,
                },
            );

            exchange_result.access_token
        }
    };

    // Build request
    let http = http_client()?;

    let method: reqwest::Method =
        args.method
            .to_uppercase()
            .parse()
            .map_err(|_| AppError::InvalidInput {
                message: format!("Invalid HTTP method: {}", args.method),
            })?;

    let mut request = http
        .request(method, parsed_url.as_str())
        .header("Authorization", format!("Bearer {}", token));

    // Add custom headers
    for h in &args.headers {
        let colon_idx = h.find(':').ok_or_else(|| AppError::InvalidInput {
            message: format!("Invalid header format: \"{}\". Use \"Key: Value\".", h),
        })?;
        let key = h[..colon_idx].trim();
        let value = h[colon_idx + 1..].trim();
        request = request.header(key, value);
    }

    // Add body
    if let Some(ref data) = args.data {
        request = request.body(data.clone());
    } else if let Some(ref data_file) = args.data_file {
        let body = std::fs::read_to_string(data_file).map_err(|e| AppError::InvalidInput {
            message: format!("Failed to read file {}: {}", data_file, e),
        })?;
        request = request.body(body);
    }

    // Execute
    let response = request.send().await.map_err(|e| {
        if e.is_timeout() || e.is_connect() {
            AppError::NetworkError {
                message: format!("Request failed: {}", e),
            }
        } else {
            AppError::ServiceError {
                message: format!("Request failed: {}", e),
            }
        }
    })?;

    let status = response.status();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let body_text = response.text().await.unwrap_or_default();

    let body_json: serde_json::Value = if content_type.contains("application/json") {
        serde_json::from_str(&body_text).unwrap_or(serde_json::Value::String(body_text.clone()))
    } else {
        serde_json::Value::String(body_text.clone())
    };

    if !status.is_success() {
        output(
            serde_json::json!({
                "status": status.as_u16(),
                "statusText": status.canonical_reason().unwrap_or(""),
                "body": body_json,
            }),
            &if content_type.contains("application/json") {
                serde_json::to_string_pretty(&body_json).unwrap_or(body_text)
            } else {
                body_text
            },
            json_mode,
        );
        return Err(AppError::ServiceError {
            message: format!("HTTP {}", status),
        }
        .into());
    }

    let display_text = if content_type.contains("application/json") {
        serde_json::to_string_pretty(&body_json).unwrap_or(body_text)
    } else {
        body_text
    };

    output(
        serde_json::json!({
            "status": status.as_u16(),
            "body": body_json,
        }),
        &display_text,
        json_mode,
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_check_exact_match() {
        let domains = vec!["api.github.com".to_string()];
        assert!(is_domain_allowed("api.github.com", &domains));
        assert!(!is_domain_allowed("evil.com", &domains));
    }

    #[test]
    fn domain_check_wildcard() {
        let domains = vec!["*.googleapis.com".to_string()];
        assert!(is_domain_allowed("www.googleapis.com", &domains));
        assert!(is_domain_allowed("a.b.googleapis.com", &domains));
        assert!(!is_domain_allowed("googleapis.com", &domains));
        assert!(!is_domain_allowed("evil.com", &domains));
    }

    #[test]
    fn domain_check_case_insensitive() {
        let domains = vec!["API.GitHub.com".to_string()];
        assert!(is_domain_allowed("api.github.com", &domains));
        assert!(is_domain_allowed("API.GITHUB.COM", &domains));
    }

    #[test]
    fn domain_check_mixed_list() {
        let domains = vec!["slack.com".to_string(), "*.slack.com".to_string()];
        assert!(is_domain_allowed("slack.com", &domains));
        assert!(is_domain_allowed("api.slack.com", &domains));
        assert!(!is_domain_allowed("evil.slack.com.attacker.com", &domains));
    }

    #[test]
    fn domain_check_empty_list_rejects() {
        assert!(!is_domain_allowed("api.github.com", &[]));
    }

    #[test]
    fn domain_check_multiple_allowed() {
        let domains = vec!["api.github.com".to_string(), "api.slack.com".to_string()];
        assert!(is_domain_allowed("api.slack.com", &domains));
        assert!(is_domain_allowed("api.github.com", &domains));
        assert!(!is_domain_allowed("evil.com", &domains));
    }

    #[test]
    fn domain_check_rejects_partial_match() {
        let domains = vec!["api.github.com".to_string()];
        assert!(!is_domain_allowed("notapi.github.com", &domains));
    }

    #[test]
    fn domain_check_rejects_suffix_match() {
        let domains = vec!["api.github.com".to_string()];
        assert!(!is_domain_allowed("github.com", &domains));
    }

    #[test]
    fn domain_check_wildcard_deeply_nested() {
        let domains = vec!["*.googleapis.com".to_string()];
        assert!(is_domain_allowed("a.b.c.googleapis.com", &domains));
    }
}
