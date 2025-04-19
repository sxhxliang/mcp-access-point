use std::{collections::HashMap, net::SocketAddr};

use regex::Regex;
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};
use pingora::{Error, ErrorType::*, OrErr, Result};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamConfig {
    pub upstream: Option<String>,
    pub ip: Option<String>,
    pub port: Option<u16>,
    #[serde(default)]
    pub headers: Option<HashMap<String, String>>,
}

impl UpstreamConfig {
    pub fn to_socket_addrs(&self) -> Result<SocketAddr> {
        Ok(SocketAddr::new(
            self.ip.clone().unwrap()
                .parse()
                .or_err_with(ReadError, || format!("Invalid ip address: {}", self.ip.clone().unwrap()))?,
            self.port.unwrap(),
        ))
    }
    pub fn get_addr(&self) -> String {
        self.upstream.as_ref().map_or_else(
            || format!("{}:{}", self.ip.clone().unwrap(), self.port.unwrap()),
            |addr| addr.to_string()
        )
    }

    pub fn parse_addr(addr: &str) -> Result<Self, String> {
        let binding = addr.replace("http://", "").replace("https://", "");
        let parts: Vec<&str> = binding.split(':').collect();

        if parts.len() != 2 {
            return Err(format!("Invalid address format: {}", addr));
        }

        let ip = parts[0].to_string();
        let port = parts[1]
            .parse::<u16>()
            .map_err(|_| format!("Invalid port number: {}", parts[1]))?;

        Ok(UpstreamConfig {
            upstream: Some(addr.to_string()),
            ip: Some(ip),
            port: Some(port),
            headers: None,
        })
    }
    pub fn get_headers(&self) -> HashMap<String, String> {
        self.headers.clone().unwrap_or_default()
    }
    pub fn with_headers(self, headers: HashMap<String, String>) -> Self {
        Self {
            headers: Some(headers),
            ..self
        }
    }
}

impl Default for UpstreamConfig {
    fn default() -> Self {
        Self {
            upstream: Some("127.0.0.1:8090".to_string()),
            ip: Some("127.0.0.1".to_string()),
            port: Some(8090),
            headers: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Health {
    #[serde(default = "Health::default_interval")]
    pub interval: u32,
    #[serde(default = "Health::default_http_statuses")]
    pub http_statuses: Vec<u32>,
    #[serde(default = "Health::default_successes")]
    pub successes: u32,
}

impl Health {
    fn default_interval() -> u32 {
        1
    }

    fn default_http_statuses() -> Vec<u32> {
        vec![200, 302]
    }

    fn default_successes() -> u32 {
        2
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]

pub struct Unhealthy {
    #[serde(default = "Unhealthy::default_http_failures")]
    pub http_failures: u32,
    #[serde(default = "Unhealthy::default_tcp_failures")]
    pub tcp_failures: u32,
}

impl Unhealthy {
    fn default_http_failures() -> u32 {
        5
    }

    fn default_tcp_failures() -> u32 {
        2
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct HealthCheck {
    // only support passive check for now
    #[validate(nested)]
    pub active: ActiveCheck,
}



#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActiveCheckType {
    TCP,
    #[default]
    HTTP,
    HTTPS,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct ActiveCheck {
    #[serde(default)]
    pub r#type: ActiveCheckType,
    #[serde(default = "ActiveCheck::default_timeout")]
    pub timeout: u32,
    #[serde(default = "ActiveCheck::default_http_path")]
    pub http_path: String,
    pub host: Option<String>,
    pub port: Option<u32>,
    #[serde(default = "ActiveCheck::default_https_verify_certificate")]
    pub https_verify_certificate: bool,
    #[serde(default)]
    pub req_headers: Vec<String>,
    pub healthy: Option<Health>,
    #[validate(nested)]
    pub unhealthy: Option<Unhealthy>,
}
impl ActiveCheck {
    fn default_timeout() -> u32 {
        1
    }

    fn default_http_path() -> String {
        "/".to_string()
    }

    fn default_https_verify_certificate() -> bool {
        true
    }
}

#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpstreamPassHost {
    #[default]
    PASS,
    REWRITE,
}

#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpstreamScheme {
    #[default]
    HTTP,
    HTTPS,
    GRPC,
    GRPCS,
}

#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpstreamHashOn {
    #[default]
    VARS,
    HEAD,
    COOKIE,
}

#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SelectionType {
    #[default]
    RoundRobin,
    Random,
    Fnv,
    Ketama,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct Timeout {
    pub connect: u64,
    pub send: u64,
    pub read: u64,
}


#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[validate(schema(function = "Upstream::validate_upstream_host"))]
pub struct Upstream {
    #[serde(default)]
    pub id: String,
    pub retries: Option<u32>,
    pub retry_timeout: Option<u64>,
    #[validate(nested)]
    pub timeout: Option<Timeout>,
    #[validate(length(min = 1), custom(function = "Upstream::validate_nodes_keys"))]
    pub nodes: HashMap<String, u32>,
    #[serde(default)]
    pub r#type: SelectionType,
    #[validate(nested)]
    pub checks: Option<HealthCheck>,
    #[serde(default)]
    pub hash_on: UpstreamHashOn,
    #[serde(default = "Upstream::default_key")]
    pub key: String,
    #[serde(default)]
    pub scheme: UpstreamScheme,
    #[serde(default)]
    pub pass_host: UpstreamPassHost,
    pub upstream_host: Option<String>,
}

impl Upstream {
    fn default_key() -> String {
        "uri".to_string()
    }

    fn validate_upstream_host(&self) -> Result<(), ValidationError> {
        if self.pass_host == UpstreamPassHost::REWRITE {
            self.upstream_host.as_ref().map_or_else(
                || Err(ValidationError::new("upstream_host_required_for_rewrite")),
                |_| Ok(()),
            )
        } else {
            Ok(())
        }
    }

    // Custom validation function for `nodes` keys
    fn validate_nodes_keys(nodes: &HashMap<String, u32>) -> Result<(), ValidationError> {
        let re =
            Regex::new(r"(?i)^(?:(?:\d{1,3}\.){3}\d{1,3}|\[[0-9a-f:]+\]|[a-z0-9.-]+)(?::\d+)?$")
                .unwrap();

        for key in nodes.keys() {
            if !re.is_match(key) {
                let mut err = ValidationError::new("invalid_node_key");
                err.add_param("key".into(), key);
                return Err(err);
            }
        }

        Ok(())
    }
}
