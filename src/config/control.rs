use std::net::SocketAddr;

use pingora_error::Result;
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};
/// Configuration for the control server.
#[derive(Clone, Debug, Serialize, Deserialize, Validate)]
pub struct Admin {
    /// The address to bind the control server to.
    pub address: SocketAddr,
    /// The API key for authentication.
    pub api_key: String,
}
/// Configuration for Prometheus metrics.
#[derive(Clone, Debug, Serialize, Deserialize, Validate)]
pub struct Prometheus {
    /// The address to listen on for Prometheus metrics.
    pub address: SocketAddr,
}

/// Configuration for Sentry error tracking.
#[derive(Clone, Debug, Serialize, Deserialize, Validate)]
pub struct Sentry {
    /// The DSN for Sentry.
    pub dsn: String,
}

/// Configuration for logging.
#[derive(Clone, Debug, Serialize, Deserialize, Validate)]
pub struct Log {
    /// The path to the log file.
    #[validate(length(min = 1), custom(function = "Log::validate_path"))]
    pub path: String,
}

impl Log {
    fn validate_path(path: &str) -> Result<(), ValidationError> {
        if path.contains('\0') || path.trim().is_empty() {
            return Err(ValidationError::new("Invalid log file path"));
        }
        Ok(())
    }
}
