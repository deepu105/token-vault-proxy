use crate::utils::exit_codes::*;
use thiserror::Error;

/// Application-level errors that map to exit codes.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("{message}")]
    General { message: String },

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
            AppError::General { .. } => EXIT_GENERAL,
            AppError::InvalidInput { .. } => EXIT_INVALID_INPUT,
            AppError::AuthRequired { .. } => EXIT_AUTH_REQUIRED,
            AppError::AuthzRequired { .. } => EXIT_AUTHZ_REQUIRED,
            AppError::ServiceError { .. } => EXIT_SERVICE_ERROR,
            AppError::NetworkError { .. } => EXIT_NETWORK_ERROR,
        }
    }

    pub fn error_code(&self) -> &'static str {
        match self {
            AppError::General { .. } => "general_error",
            AppError::InvalidInput { .. } => "invalid_input",
            AppError::AuthRequired { .. } => "auth_required",
            AppError::AuthzRequired { .. } => "authz_required",
            AppError::ServiceError { .. } => "service_error",
            AppError::NetworkError { .. } => "network_error",
        }
    }
}
