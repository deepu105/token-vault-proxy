// Exit code constants matching auth0-tv conventions.

/// General / unexpected error
pub const EXIT_GENERAL: i32 = 1;

/// Invalid input or usage
pub const EXIT_INVALID_INPUT: i32 = 2;

/// Authentication required (not logged in or token expired)
pub const EXIT_AUTH_REQUIRED: i32 = 3;

/// Authorization required (service not connected)
pub const EXIT_AUTHZ_REQUIRED: i32 = 4;

/// Upstream service error (e.g. Gmail API failure)
pub const EXIT_SERVICE_ERROR: i32 = 5;

/// Network error (unreachable host, timeout)
pub const EXIT_NETWORK_ERROR: i32 = 6;
