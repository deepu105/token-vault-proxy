use crate::utils::exit_codes::*;
use thiserror::Error;

/// Application-level errors that map to exit codes.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("{message}")]
    InvalidInput { message: String },

    #[error("{message}")]
    AuthRequired { message: String },

    #[error("{message}")]
    AuthzRequired { message: String },

    #[error("{message}")]
    ServiceError { message: String },

    #[error("{message}")]
    NetworkError { message: String },
}

impl AppError {
    pub fn exit_code(&self) -> i32 {
        match self {
            AppError::InvalidInput { .. } => EXIT_INVALID_INPUT,
            AppError::AuthRequired { .. } => EXIT_AUTH_REQUIRED,
            AppError::AuthzRequired { .. } => EXIT_AUTHZ_REQUIRED,
            AppError::ServiceError { .. } => EXIT_SERVICE_ERROR,
            AppError::NetworkError { .. } => EXIT_NETWORK_ERROR,
        }
    }

    pub fn error_code(&self) -> &'static str {
        match self {
            AppError::InvalidInput { .. } => "invalid_input",
            AppError::AuthRequired { .. } => "auth_required",
            AppError::AuthzRequired { .. } => "authz_required",
            AppError::ServiceError { .. } => "service_error",
            AppError::NetworkError { .. } => "network_error",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_code_constants_match_plan() {
        assert_eq!(EXIT_GENERAL, 1);
        assert_eq!(EXIT_INVALID_INPUT, 2);
        assert_eq!(EXIT_AUTH_REQUIRED, 3);
        assert_eq!(EXIT_AUTHZ_REQUIRED, 4);
        assert_eq!(EXIT_SERVICE_ERROR, 5);
        assert_eq!(EXIT_NETWORK_ERROR, 6);
    }

    #[test]
    fn app_error_exit_code_mapping() {
        assert_eq!(AppError::InvalidInput { message: "x".into() }.exit_code(), 2);
        assert_eq!(AppError::AuthRequired { message: "x".into() }.exit_code(), 3);
        assert_eq!(AppError::AuthzRequired { message: "x".into() }.exit_code(), 4);
        assert_eq!(AppError::ServiceError { message: "x".into() }.exit_code(), 5);
        assert_eq!(AppError::NetworkError { message: "x".into() }.exit_code(), 6);
    }

    #[test]
    fn app_error_error_code_strings() {
        assert_eq!(AppError::InvalidInput { message: "x".into() }.error_code(), "invalid_input");
        assert_eq!(AppError::AuthRequired { message: "x".into() }.error_code(), "auth_required");
        assert_eq!(AppError::AuthzRequired { message: "x".into() }.error_code(), "authz_required");
        assert_eq!(AppError::ServiceError { message: "x".into() }.error_code(), "service_error");
        assert_eq!(AppError::NetworkError { message: "x".into() }.error_code(), "network_error");
    }

    #[test]
    fn app_error_display_message() {
        let err = AppError::ServiceError { message: "something broke".into() };
        assert_eq!(format!("{}", err), "something broke");
    }
}
