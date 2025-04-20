use std::net::SocketAddr;

use pingora_error::Result;
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};


#[derive(Clone, Debug, Serialize, Deserialize, Validate)]
pub struct Admin {
    pub address: SocketAddr,
    pub api_key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Validate)]
pub struct Prometheus {
    pub address: SocketAddr,
}

#[derive(Clone, Debug, Serialize, Deserialize, Validate)]
pub struct Sentry {
    pub dsn: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Validate)]
pub struct Log {
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