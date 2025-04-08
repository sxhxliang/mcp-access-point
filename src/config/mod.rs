use core::str;
use std::sync::RwLock;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

pub const SERVER_NAME: &str = "mcp_proxy";
pub const SERVER_VERSION: &str = "1.5";
pub const CLIENT_SSE_ENDPOINT: &str = "/sse";
pub const CLIENT_MESSAGE_ENDPOINT: &str = "/messages/";
pub const ERROR_MESSAGE: &str = "Unable to fetch data for this mcp server.";
pub const SERVER_WITH_AUTH: bool = false;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamConfig {
    pub ip: String,
    pub port: u16,
}

impl Default for UpstreamConfig {
    fn default() -> Self {
        Self {
            ip: "127.0.0.1".to_string(),
            port: 8090,
        }
    }
}
impl UpstreamConfig {
    pub fn parse(addr: &str) -> Result<Self, String> {
        let binding = addr.replace("http://", "").replace("https://", "");
        let parts: Vec<&str> = binding.split(':').collect();

        if parts.len() != 2 {
            return Err(format!("Invalid address format: {}", addr));
        }

        let ip = parts[0].to_string();
        let port = parts[1]
            .parse::<u16>()
            .map_err(|_| format!("Invalid port number: {}", parts[1]))?;

        Ok(UpstreamConfig { ip, port })
    }
}

pub static UPSTREAM_CONFIG: Lazy<RwLock<UpstreamConfig>> =
    Lazy::new(|| RwLock::new(UpstreamConfig::default()));

#[derive(Debug, Clone, Default)]
pub struct MCPServerConfig {
    pub server_name: String,
    pub server_version: String,
    pub client_sse_endpoint: String,
    pub client_message_endpoint: String,
    pub error_message: String,
    pub admin: Admin,
}

#[derive(Debug, Clone, Default)]
pub struct Admin {
    pub address: String,
    pub api_key: String,
}
