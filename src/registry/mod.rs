// Provider registry: aliases, connections, default scopes, allowed domains.
//
// Two-level hierarchy: providers contain services. Each provider has a canonical
// connection name (e.g. `google-oauth2`), friendly aliases (e.g. `google`), and
// one or more services (e.g. `gmail`, `calendar`) with per-service scopes and
// allowed domains.
//
// All lookups are case-insensitive.

/// A service within a provider (e.g. "gmail" under "google-oauth2").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceEntry {
    /// Service name, e.g. "gmail", "calendar", "slack".
    pub name: &'static str,
    /// OAuth scopes required for this service.
    pub scopes: &'static [&'static str],
    /// Allowed domains for the `fetch` command.
    pub allowed_domains: &'static [&'static str],
}

/// A provider entry (e.g. "google-oauth2").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderEntry {
    /// Auth0 connection identifier, e.g. "google-oauth2".
    pub connection: &'static str,
    /// Friendly aliases, e.g. ["google"].
    pub aliases: &'static [&'static str],
    /// Services under this provider.
    pub services: &'static [ServiceEntry],
}

/// Result of `resolve_any` — attempts provider match first, then service match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution<'a> {
    /// Matched a provider by connection name or alias.
    ProviderMatch(&'a ProviderEntry),
    /// Matched a service by service name.
    ServiceMatch(&'a ProviderEntry, &'a ServiceEntry),
    /// No match found — the string is passed through for the caller to handle.
    Unknown(String),
}

// ---------------------------------------------------------------------------
// Static registry data
// ---------------------------------------------------------------------------

static GMAIL_SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/gmail.readonly",
    "https://www.googleapis.com/auth/gmail.send",
    "https://www.googleapis.com/auth/gmail.compose",
    "https://www.googleapis.com/auth/gmail.modify",
    "https://www.googleapis.com/auth/gmail.labels",
];

static CALENDAR_SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/calendar.readonly",
    "https://www.googleapis.com/auth/calendar.events",
];

static SLACK_SCOPES: &[&str] = &[
    "channels:read",
    "channels:history",
    "groups:read",
    "groups:history",
    "chat:write",
    "search:read.public",
    "reactions:write",
    "users:read",
    "users.profile:read",
];

static GOOGLE_DOMAINS: &[&str] = &["*.googleapis.com"];
static GITHUB_DOMAINS: &[&str] = &["api.github.com"];
static SLACK_DOMAINS: &[&str] = &["slack.com", "*.slack.com"];

static PROVIDERS: &[ProviderEntry] = &[
    ProviderEntry {
        connection: "google-oauth2",
        aliases: &["google"],
        services: &[
            ServiceEntry {
                name: "gmail",
                scopes: GMAIL_SCOPES,
                allowed_domains: GOOGLE_DOMAINS,
            },
            ServiceEntry {
                name: "calendar",
                scopes: CALENDAR_SCOPES,
                allowed_domains: GOOGLE_DOMAINS,
            },
        ],
    },
    ProviderEntry {
        connection: "github",
        aliases: &["github"],
        services: &[ServiceEntry {
            name: "github",
            scopes: &[],
            allowed_domains: GITHUB_DOMAINS,
        }],
    },
    ProviderEntry {
        connection: "sign-in-with-slack",
        aliases: &["slack"],
        services: &[ServiceEntry {
            name: "slack",
            scopes: SLACK_SCOPES,
            allowed_domains: SLACK_DOMAINS,
        }],
    },
];

// ---------------------------------------------------------------------------
// Lookup functions
// ---------------------------------------------------------------------------

/// Resolve an input string to a provider by connection name or alias (case-insensitive).
///
/// Service names like "gmail" do **not** match here.
pub fn resolve_provider(input: &str) -> Option<&'static ProviderEntry> {
    let lower = input.to_lowercase();
    PROVIDERS
        .iter()
        .find(|p| p.connection == lower || p.aliases.iter().any(|a| a.to_lowercase() == lower))
}

/// Resolve an input string to a service by service name (case-insensitive).
///
/// Returns both the parent provider and the matched service.
pub fn resolve_service(input: &str) -> Option<(&'static ProviderEntry, &'static ServiceEntry)> {
    let lower = input.to_lowercase();
    for provider in PROVIDERS {
        for service in provider.services {
            if service.name.to_lowercase() == lower {
                return Some((provider, service));
            }
        }
    }
    None
}

/// Resolve an input string by trying provider first, then service.
///
/// Returns a `Resolution` enum indicating whether the match was a provider,
/// a service, or unknown. Unknown inputs are passed through so callers can
/// forward them to Auth0 directly.
pub fn resolve_any(input: &str) -> Resolution<'static> {
    if let Some(provider) = resolve_provider(input) {
        return Resolution::ProviderMatch(provider);
    }
    if let Some((provider, service)) = resolve_service(input) {
        return Resolution::ServiceMatch(provider, service);
    }
    Resolution::Unknown(input.to_string())
}

/// Get the union of all service scopes under a provider.
///
/// Returns an empty vec if the provider is not found.
pub fn get_all_provider_scopes(provider_connection: &str) -> Vec<&'static str> {
    let lower = provider_connection.to_lowercase();
    let Some(provider) = PROVIDERS.iter().find(|p| p.connection == lower) else {
        return Vec::new();
    };
    let mut scopes = Vec::new();
    for service in provider.services {
        for scope in service.scopes {
            if !scopes.contains(scope) {
                scopes.push(*scope);
            }
        }
    }
    scopes
}

/// Get scopes for a specific service under a provider.
///
/// Returns an empty vec if the provider or service is not found.
pub fn get_service_scopes(provider_connection: &str, service_name: &str) -> Vec<&'static str> {
    let prov_lower = provider_connection.to_lowercase();
    let svc_lower = service_name.to_lowercase();
    let Some(provider) = PROVIDERS.iter().find(|p| p.connection == prov_lower) else {
        return Vec::new();
    };
    provider
        .services
        .iter()
        .find(|s| s.name.to_lowercase() == svc_lower)
        .map(|s| s.scopes.to_vec())
        .unwrap_or_default()
}

/// Get allowed domains for a specific service (if given) or the union of all
/// services under a provider.
///
/// Returns an empty vec if the provider is not found.
pub fn get_allowed_domains(
    provider_connection: &str,
    service_name: Option<&str>,
) -> Vec<&'static str> {
    let prov_lower = provider_connection.to_lowercase();
    let Some(provider) = PROVIDERS.iter().find(|p| p.connection == prov_lower) else {
        return Vec::new();
    };

    if let Some(svc) = service_name {
        let svc_lower = svc.to_lowercase();
        return provider
            .services
            .iter()
            .find(|s| s.name.to_lowercase() == svc_lower)
            .map(|s| s.allowed_domains.to_vec())
            .unwrap_or_default();
    }

    // Union of all service domains under this provider (deduplicated).
    let mut domains = Vec::new();
    for service in provider.services {
        for domain in service.allowed_domains {
            if !domains.contains(domain) {
                domains.push(*domain);
            }
        }
    }
    domains
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- resolve_any --------------------------------------------------------

    #[test]
    fn resolve_any_google_alias_returns_provider_match() {
        match resolve_any("google") {
            Resolution::ProviderMatch(p) => {
                assert_eq!(p.connection, "google-oauth2");
            }
            other => panic!("expected ProviderMatch, got {:?}", other),
        }
    }

    #[test]
    fn resolve_any_google_oauth2_returns_provider_match() {
        match resolve_any("google-oauth2") {
            Resolution::ProviderMatch(p) => {
                assert_eq!(p.connection, "google-oauth2");
            }
            other => panic!("expected ProviderMatch, got {:?}", other),
        }
    }

    #[test]
    fn resolve_any_gmail_returns_service_match() {
        match resolve_any("gmail") {
            Resolution::ServiceMatch(provider, service) => {
                assert_eq!(provider.connection, "google-oauth2");
                assert_eq!(service.name, "gmail");
            }
            other => panic!("expected ServiceMatch, got {:?}", other),
        }
    }

    #[test]
    fn resolve_any_calendar_returns_service_match() {
        match resolve_any("calendar") {
            Resolution::ServiceMatch(provider, service) => {
                assert_eq!(provider.connection, "google-oauth2");
                assert_eq!(service.name, "calendar");
            }
            other => panic!("expected ServiceMatch, got {:?}", other),
        }
    }

    #[test]
    fn resolve_any_unknown_returns_unknown() {
        match resolve_any("dropbox") {
            Resolution::Unknown(s) => assert_eq!(s, "dropbox"),
            other => panic!("expected Unknown, got {:?}", other),
        }
    }

    // -- resolve_provider ---------------------------------------------------

    #[test]
    fn resolve_provider_by_connection_name() {
        let p = resolve_provider("google-oauth2").expect("should find provider");
        assert_eq!(p.connection, "google-oauth2");
    }

    #[test]
    fn resolve_provider_by_alias() {
        let p = resolve_provider("google").expect("should find provider");
        assert_eq!(p.connection, "google-oauth2");
    }

    #[test]
    fn resolve_provider_case_insensitive() {
        assert!(resolve_provider("Google").is_some());
        assert!(resolve_provider("GOOGLE-OAUTH2").is_some());
        assert!(resolve_provider("GitHub").is_some());
        assert!(resolve_provider("Slack").is_some());
    }

    #[test]
    fn resolve_provider_unknown() {
        assert!(resolve_provider("dropbox").is_none());
    }

    #[test]
    fn resolve_provider_service_name_does_not_match() {
        // "gmail" is a service, not a provider alias
        assert!(resolve_provider("gmail").is_none());
        assert!(resolve_provider("calendar").is_none());
    }

    // -- resolve_service ----------------------------------------------------

    #[test]
    fn resolve_service_gmail() {
        let (provider, service) = resolve_service("gmail").expect("should find gmail");
        assert_eq!(provider.connection, "google-oauth2");
        assert_eq!(service.name, "gmail");
    }

    #[test]
    fn resolve_service_calendar() {
        let (provider, service) = resolve_service("calendar").expect("should find calendar");
        assert_eq!(provider.connection, "google-oauth2");
        assert_eq!(service.name, "calendar");
    }

    #[test]
    fn resolve_service_github() {
        let (provider, service) = resolve_service("github").expect("should find github");
        assert_eq!(provider.connection, "github");
        assert_eq!(service.name, "github");
    }

    #[test]
    fn resolve_service_slack() {
        let (provider, service) = resolve_service("slack").expect("should find slack");
        assert_eq!(provider.connection, "sign-in-with-slack");
        assert_eq!(service.name, "slack");
    }

    #[test]
    fn resolve_service_case_insensitive() {
        assert!(resolve_service("Gmail").is_some());
        assert!(resolve_service("CALENDAR").is_some());
        assert!(resolve_service("Slack").is_some());
    }

    #[test]
    fn resolve_service_unknown() {
        assert!(resolve_service("dropbox").is_none());
    }

    // -- get_all_provider_scopes --------------------------------------------

    #[test]
    fn get_all_provider_scopes_google() {
        let scopes = get_all_provider_scopes("google-oauth2");
        // gmail (5) + calendar (2) = 7 total scopes
        assert_eq!(scopes.len(), 7);
        assert!(scopes.contains(&"https://www.googleapis.com/auth/gmail.readonly"));
        assert!(scopes.contains(&"https://www.googleapis.com/auth/gmail.send"));
        assert!(scopes.contains(&"https://www.googleapis.com/auth/gmail.compose"));
        assert!(scopes.contains(&"https://www.googleapis.com/auth/gmail.modify"));
        assert!(scopes.contains(&"https://www.googleapis.com/auth/gmail.labels"));
        assert!(scopes.contains(&"https://www.googleapis.com/auth/calendar.readonly"));
        assert!(scopes.contains(&"https://www.googleapis.com/auth/calendar.events"));
    }

    #[test]
    fn get_all_provider_scopes_github_empty() {
        let scopes = get_all_provider_scopes("github");
        assert!(
            scopes.is_empty(),
            "github uses fine-grained auth, no default scopes"
        );
    }

    #[test]
    fn get_all_provider_scopes_slack() {
        let scopes = get_all_provider_scopes("sign-in-with-slack");
        assert_eq!(scopes.len(), 9);
        assert!(scopes.contains(&"channels:read"));
        assert!(scopes.contains(&"chat:write"));
        assert!(scopes.contains(&"users.profile:read"));
    }

    #[test]
    fn get_all_provider_scopes_unknown_returns_empty() {
        let scopes = get_all_provider_scopes("dropbox");
        assert!(scopes.is_empty());
    }

    // -- get_service_scopes -------------------------------------------------

    #[test]
    fn get_service_scopes_gmail() {
        let scopes = get_service_scopes("google-oauth2", "gmail");
        assert_eq!(scopes.len(), 5);
        assert!(scopes.contains(&"https://www.googleapis.com/auth/gmail.readonly"));
        assert!(scopes.contains(&"https://www.googleapis.com/auth/gmail.send"));
        assert!(scopes.contains(&"https://www.googleapis.com/auth/gmail.compose"));
        assert!(scopes.contains(&"https://www.googleapis.com/auth/gmail.modify"));
        assert!(scopes.contains(&"https://www.googleapis.com/auth/gmail.labels"));
    }

    #[test]
    fn get_service_scopes_calendar() {
        let scopes = get_service_scopes("google-oauth2", "calendar");
        assert_eq!(scopes.len(), 2);
        assert!(scopes.contains(&"https://www.googleapis.com/auth/calendar.readonly"));
        assert!(scopes.contains(&"https://www.googleapis.com/auth/calendar.events"));
    }

    #[test]
    fn get_service_scopes_wrong_service_returns_empty() {
        let scopes = get_service_scopes("google-oauth2", "slack");
        assert!(scopes.is_empty());
    }

    #[test]
    fn get_service_scopes_unknown_provider_returns_empty() {
        let scopes = get_service_scopes("dropbox", "gmail");
        assert!(scopes.is_empty());
    }

    // -- get_allowed_domains ------------------------------------------------

    #[test]
    fn get_allowed_domains_google_gmail() {
        let domains = get_allowed_domains("google-oauth2", Some("gmail"));
        assert_eq!(domains, vec!["*.googleapis.com"]);
    }

    #[test]
    fn get_allowed_domains_google_calendar() {
        let domains = get_allowed_domains("google-oauth2", Some("calendar"));
        assert_eq!(domains, vec!["*.googleapis.com"]);
    }

    #[test]
    fn get_allowed_domains_google_all() {
        let domains = get_allowed_domains("google-oauth2", None);
        // Both gmail and calendar share the same domain, so union is still 1 entry.
        assert_eq!(domains, vec!["*.googleapis.com"]);
    }

    #[test]
    fn get_allowed_domains_github() {
        let domains = get_allowed_domains("github", Some("github"));
        assert_eq!(domains, vec!["api.github.com"]);
    }

    #[test]
    fn get_allowed_domains_slack_none() {
        let domains = get_allowed_domains("sign-in-with-slack", None);
        assert_eq!(domains, vec!["slack.com", "*.slack.com"]);
    }

    #[test]
    fn get_allowed_domains_slack_service() {
        let domains = get_allowed_domains("sign-in-with-slack", Some("slack"));
        assert_eq!(domains, vec!["slack.com", "*.slack.com"]);
    }

    #[test]
    fn get_allowed_domains_unknown_provider() {
        let domains = get_allowed_domains("dropbox", None);
        assert!(domains.is_empty());
    }

    // -- Scope parity with auth0-tv's service-registry.ts -------------------

    #[test]
    fn gmail_scopes_match_typescript_registry() {
        let scopes = get_service_scopes("google-oauth2", "gmail");
        let expected = vec![
            "https://www.googleapis.com/auth/gmail.readonly",
            "https://www.googleapis.com/auth/gmail.send",
            "https://www.googleapis.com/auth/gmail.compose",
            "https://www.googleapis.com/auth/gmail.modify",
            "https://www.googleapis.com/auth/gmail.labels",
        ];
        assert_eq!(scopes, expected);
    }

    #[test]
    fn calendar_scopes_match_typescript_registry() {
        let scopes = get_service_scopes("google-oauth2", "calendar");
        let expected = vec![
            "https://www.googleapis.com/auth/calendar.readonly",
            "https://www.googleapis.com/auth/calendar.events",
        ];
        assert_eq!(scopes, expected);
    }

    #[test]
    fn github_scopes_match_typescript_registry() {
        let scopes = get_service_scopes("github", "github");
        assert!(scopes.is_empty(), "github uses fine-grained auth");
    }

    #[test]
    fn slack_scopes_match_typescript_registry() {
        let scopes = get_service_scopes("sign-in-with-slack", "slack");
        let expected = vec![
            "channels:read",
            "channels:history",
            "groups:read",
            "groups:history",
            "chat:write",
            "search:read.public",
            "reactions:write",
            "users:read",
            "users.profile:read",
        ];
        assert_eq!(scopes, expected);
    }

    // -- Provider hierarchy structure ---------------------------------------

    #[test]
    fn google_provider_has_two_services() {
        let p = resolve_provider("google-oauth2").unwrap();
        assert_eq!(p.services.len(), 2);
        let names: Vec<&str> = p.services.iter().map(|s| s.name).collect();
        assert!(names.contains(&"gmail"));
        assert!(names.contains(&"calendar"));
    }

    #[test]
    fn github_provider_has_one_service() {
        let p = resolve_provider("github").unwrap();
        assert_eq!(p.services.len(), 1);
        assert_eq!(p.services[0].name, "github");
    }

    #[test]
    fn slack_provider_has_one_service() {
        let p = resolve_provider("sign-in-with-slack").unwrap();
        assert_eq!(p.services.len(), 1);
        assert_eq!(p.services[0].name, "slack");
    }

    // -- Edge cases ---------------------------------------------------------

    #[test]
    fn github_alias_resolves_as_provider_not_service() {
        // "github" is both an alias and a service name. Provider match wins.
        match resolve_any("github") {
            Resolution::ProviderMatch(p) => {
                assert_eq!(p.connection, "github");
            }
            other => panic!("expected ProviderMatch, got {:?}", other),
        }
    }

    #[test]
    fn slack_alias_resolves_as_provider_not_service() {
        // "slack" is both an alias and a service name. Provider match wins.
        match resolve_any("slack") {
            Resolution::ProviderMatch(p) => {
                assert_eq!(p.connection, "sign-in-with-slack");
            }
            other => panic!("expected ProviderMatch, got {:?}", other),
        }
    }

    #[test]
    fn resolve_any_empty_string_returns_unknown() {
        match resolve_any("") {
            Resolution::Unknown(s) => assert_eq!(s, ""),
            other => panic!("expected Unknown, got {:?}", other),
        }
    }
}
