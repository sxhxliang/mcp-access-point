pub mod upstream;
pub use upstream::*;
use validator::Validate;

use core::str;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::{fs, sync::RwLock};
// use serde_yaml::Value as YamlValue;

use pingora::server::configuration::{Opt, ServerConf};
use pingora::{Error, ErrorType::*, OrErr, Result};
// use validator::Validate;

pub const SERVER_NAME: &str = "mcp_proxy";
pub const SERVER_VERSION: &str = "1.5";

// 2024-11-05 specification protocol;
pub const CLIENT_SSE_ENDPOINT: &str = "/sse";
pub const CLIENT_MESSAGE_ENDPOINT: &str = "/messages/";

// 2025-03-26 specification protocol;
pub const CLIENT_STREAMABLE_HTTP_ENDPOINT: &str = "/mcp";
pub const ERROR_MESSAGE: &str = "Unable to fetch data for this mcp server.";
pub const SERVER_WITH_AUTH: bool = false;

#[derive(Default, Debug, Serialize, Deserialize, Validate)]
pub struct Config {
    #[serde(default)]
    pub pingora: ServerConf,
    pub mcps: Vec<MCPOpenAPI>,
    #[validate(nested)]
    #[serde(default)]
    pub upstreams: Vec<Upstream>,
}

// Config file load and validation
impl Config {
    // Does not have to be async until we want runtime reload
    pub fn load_from_yaml<P>(path: P) -> Result<Self>
    where
        P: AsRef<std::path::Path> + std::fmt::Display,
    {
        let conf_str = fs::read_to_string(&path).or_err_with(ReadError, || {
            format!("Unable to read conf file from {path}")
        })?;
        log::debug!("Conf file read from {path}");
        Self::from_yaml(&conf_str)
    }
    pub fn load_yaml_with_opt_override(opt: &Opt) -> Result<Self> {
        if let Some(path) = &opt.conf {
            let mut conf = Self::load_from_yaml(path)?;
            conf.pingora.merge_with_opt(opt);
            Ok(conf)
        } else {
            Error::e_explain(ReadError, "No path specified")
        }
    }
    pub fn from_yaml(conf_str: &str) -> Result<Self> {
        log::trace!("Read conf file: {conf_str}");
        let conf: Config = serde_yaml::from_str(conf_str).or_err_with(ReadError, || {
            format!("Unable to parse yaml conf {conf_str}")
        })?;

        log::trace!("Loaded conf: {conf:?}");

        // use validator to validate conf file
        // conf.validate()
        //     .or_err_with(FileReadError, || "Conf file valid failed")?;

        Ok(conf)
    }
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct MCPOpenAPI {
    pub upstream: Option<String>,
    pub upstream_config: Option<UpstreamConfig>,
    pub path: String,
}
impl MCPOpenAPI {
    pub fn parse_to_upstream_config(&self) -> Result<UpstreamConfig, String> {
        // If upstream is None, return the default configuration immediately
        if self.upstream.is_none() {
            return Ok(DEFAULT_UPSTREAM_CONFIG.read().unwrap().clone());
        }
    
        // Parse the upstream address safely
        // only if upstream_config is provided
        let upstream = self.upstream.as_ref().ok_or_else(|| "Missing upstream configuration".to_string())?;
        let mut upstream_config = UpstreamConfig::parse_addr(upstream)?;
    
        // Apply headers if upstream_config is provided
        if let Some(config) = &self.upstream_config {
            upstream_config = upstream_config.with_headers(config.get_headers());
        }

        Ok(upstream_config)
    }
}

pub static DEFAULT_UPSTREAM_CONFIG: Lazy<RwLock<UpstreamConfig>> =
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
