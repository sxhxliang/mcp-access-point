pub mod control;
pub mod etcd;
pub mod mcp;
pub mod route;
pub mod upstream;

pub use control::*;
pub use etcd::*;
pub use mcp::*;
pub use route::*;
pub use upstream::*;

use std::{collections::HashMap, fs, net::SocketAddr};

use log::{debug, trace};
use pingora::server::configuration::{Opt, ServerConf};
use pingora_error::{Error, ErrorType::*, OrErr, Result};

use serde::{Deserialize, Serialize};
use serde_yaml::Value as YamlValue;
use validator::{Validate, ValidationError};

/// Server name for the MCP Access Point API gateway.
pub const SERVER_NAME: &str = "mcp_proxy";
/// MCP server version for initialization.
pub const SERVER_VERSION: &str = "1.5";

/// 2024-11-05 specification protocol;
/// Client SSE endpoint for receiving messages from the MCP server.
pub const CLIENT_SSE_ENDPOINT: &str = "/sse";
/// Client HTTP endpoint for processing messages from the MCP client.
pub const CLIENT_MESSAGE_ENDPOINT: &str = "/messages/";

/// 2025-03-26 specification protocol;
/// Client HTTP endpoint for receiving messages from the MCP server.
pub const CLIENT_STREAMABLE_HTTP_ENDPOINT: &str = "/mcp/";
/// Default error message for when the MCP server is not reachable.
pub const ERROR_MESSAGE: &str = "Unable to fetch data for this mcp server.";
/// Whether the MCP server requires authentication.
pub const SERVER_WITH_AUTH: bool = false;

/// Trait for types with an ID field, used for unique ID validation.
pub trait Identifiable {
    /// Returns the ID of the object.
    fn id(&self) -> &str;
    /// Sets the ID of the object.
    fn set_id(&mut self, id: String);
}

macro_rules! impl_identifiable {
    ($type:ty) => {
        impl Identifiable for $type {
            fn id(&self) -> &str {
                &self.id
            }

            fn set_id(&mut self, id: String) {
                self.id = id;
            }
        }
    };
}

impl_identifiable!(Route);
impl_identifiable!(Upstream);
impl_identifiable!(Service);
impl_identifiable!(GlobalRule);
impl_identifiable!(SSL);

/// Configuration for the MCP Access Point API gateway.
#[derive(Default, Debug, Serialize, Deserialize, Validate)]
#[validate(schema(function = "Config::validate_resource_id"))]
pub struct Config {
    /// The pingora server default configuration for the MCP Access Point API gateway.
    #[serde(default)]
    pub pingora: ServerConf,
    /// The MCP Access Point API gateway configuration.
    #[validate(nested)]
    pub access_point: AccessPointConfig,
    /// mcp config
    pub mcps: Option<Vec<MCPOpenAPIConfig>>,
    /// The routes for the MCP Access Point API gateway.
    #[validate(nested)]
    #[serde(default)]
    pub routes: Vec<Route>,
    /// The upstreams for the MCP Access Point API gateway.
    #[validate(nested)]
    #[serde(default)]
    pub upstreams: Vec<Upstream>,
    /// The services for the MCP Access Point API gateway.
    #[validate(nested)]
    #[serde(default)]
    pub services: Vec<Service>,
    /// The global rules for the MCP Access Point API gateway.
    #[validate(nested)]
    #[serde(default)]
    pub global_rules: Vec<GlobalRule>,
    /// The SSLs for the MCP Access Point API gateway.
    #[validate(nested)]
    #[serde(default)]
    pub ssls: Vec<SSL>,
}

// Config file load and validation
impl Config {
    /// Does not have to be async until we want runtime reload
    /// load mcp config from yaml file
    pub fn load_from_yaml<P>(path: P) -> Result<Self>
    where
        P: AsRef<std::path::Path> + std::fmt::Display,
    {
        let conf_str = fs::read_to_string(&path).or_err_with(ReadError, || {
            format!("Unable to read conf file from {path}")
        })?;
        debug!("Conf file read from {path}");
        Self::from_yaml(&conf_str)
    }

    /// config file load entry point
    pub fn load_yaml_with_opt_override(opt: &Opt) -> Result<Self> {
        if let Some(path) = &opt.conf {
            let mut conf = Self::load_from_yaml(path)?;
            conf.merge_with_opt(opt);
            Ok(conf)
        } else {
            Error::e_explain(ReadError, "No path specified")
        }
    }
    /// load mcp config from yaml string
    pub fn from_yaml(conf_str: &str) -> Result<Self> {
        trace!("Read conf file: {conf_str}");
        let conf: Config = serde_yaml::from_str(conf_str).or_err_with(ReadError, || {
            format!("Unable to parse yaml conf {conf_str}")
        })?;

        trace!("Loaded conf: {conf:?}");

        // use validator to validate conf file
        conf.validate()
            .or_err_with(FileReadError, || "Conf file valid failed")?;

        Ok(conf)
    }

    /// serde config to yaml
    pub fn to_yaml(&self) -> String {
        serde_yaml::to_string(self).unwrap()
    }
    /// merge conf with opt
    pub fn merge_with_opt(&mut self, opt: &Opt) {
        if opt.daemon {
            self.pingora.daemon = true;
        }
    }

    fn validate_resource_id(&self) -> Result<(), ValidationError> {
        if self.upstreams.iter().any(|upstream| upstream.id.is_empty()) {
            return Err(ValidationError::new("upstream_id_required"));
        }

        if self.routes.iter().any(|route| route.id.is_empty()) {
            return Err(ValidationError::new("route_id_required"));
        }

        if self.services.iter().any(|service| service.id.is_empty()) {
            return Err(ValidationError::new("service_id_required"));
        }

        if self.global_rules.iter().any(|rule| rule.id.is_empty()) {
            return Err(ValidationError::new("global_rule_id_required"));
        }

        Ok(())
    }
}

/// Configuration for the MCP Access Point API gateway.
#[derive(Clone, Default, Debug, Serialize, Deserialize, Validate)]
pub struct AccessPointConfig {
    /// The address for the MCP Access Point API gateway.
    #[validate(length(min = 1))]
    #[validate(nested)]
    pub listeners: Vec<Listener>,
    /// The ectd configuration for the MCP Access Point API gateway.
    /// with etcd, the mcp config will be loaded from etcd.
    /// If not specified, the MCP Access Point API gateway will not use etcd.
    #[validate(nested)]
    pub etcd: Option<Etcd>,
    /// The admin configuration for the MCP Access Point API gateway.
    /// If not specified, the MCP Access Point API gateway will not use admin.
    #[validate(nested)]
    pub admin: Option<Admin>,
    /// The prometheus configuration for the MCP Access Point API gateway.
    /// If not specified, the MCP Access Point API gateway will not use prometheus.
    #[validate(nested)]
    pub prometheus: Option<Prometheus>,
    /// The sentry configuration for the MCP Access Point API gateway.
    /// If not specified, the MCP Access Point API gateway will not use sentry.
    #[validate(nested)]
    pub sentry: Option<Sentry>,
    /// The log configuration for the MCP Access Point API gateway.
    /// If not specified, the MCP Access Point API gateway will not save logs to a file.
    #[validate(nested)]
    pub log: Option<Log>,
}

/// Configuration listener for the MCP Access Point API gateway.
/// It contains the address and port to listen on.
/// It also contains the TLS configuration if the listener is using TLS.
/// If the listener is using TLS, it will use the TLS configuration to create a TLS listener.
/// If the listener is not using TLS, it will use the TCP listener.
#[derive(Clone, Debug, Serialize, Deserialize, Validate)]
#[validate(schema(function = "Listener::validate_tls_for_offer_h2"))]
pub struct Listener {
    /// The address to listen on.
    pub address: SocketAddr,
    /// The TLS configuration for the listener.
    /// If not specified, the listener will not use TLS.
    pub tls: Option<Tls>,
    /// if  true, the listener will offer HTTP/2 support.
    /// If not specified, the listener will not offer HTTP/2 support.
    #[serde(default)]
    pub offer_h2: bool,
    /// if  true, the listener will offer HTTP/2 cleartext support.
    /// If not specified, the listener will not offer HTTP/2 cleartext support.
    #[serde(default)]
    pub offer_h2c: bool,
}

impl Listener {
    fn validate_tls_for_offer_h2(&self) -> Result<(), ValidationError> {
        if self.offer_h2 && self.tls.is_none() {
            Err(ValidationError::new("tls_required_for_h2"))
        } else {
            Ok(())
        }
    }
}

/// Configuration Tls for mcp server listener.
/// It contains the path to the certificate and key file.
/// The certificate and key file are used to create a TLS listener.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tls {
    /// The path to the certificate file.
    pub cert_path: String,
    /// The path to the key file.
    pub key_path: String,
}

/// Configuration timeout for the mcp server listener.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct Timeout {
    /// The timeout for connecting to the mcp server.
    pub connect: u64,
    /// The timeout for sending data to the mcp server.
    pub send: u64,
    /// The timeout for reading data from the mcp server.
    pub read: u64,
}

/// Configuration mcp service for the mcp server.
#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[validate(schema(function = "Service::validate_upstream"))]
pub struct Service {
    /// service id for the mcp server.
    /// The id must be unique. It is used to identify the service.
    #[serde(default)]
    pub id: String,
    /// service plugins for the mcp server.
    #[serde(default)]
    pub plugins: HashMap<String, YamlValue>,
    /// upstream for the mcp server.
    /// if the upstream is not set, the service will be disabled.
    /// if the upstream_id is not set, the upstream must be set.
    pub upstream: Option<Upstream>,
    /// upstream id for the mcp server.
    /// The upstream_id must have been configured in the config.yaml .
    /// if the upstream is not set, the upstream_id must be set.
    pub upstream_id: Option<String>,
    /// hosts for the mcp server.
    #[serde(default)]
    pub hosts: Vec<String>,
}

impl Service {
    fn validate_upstream(&self) -> Result<(), ValidationError> {
        if self.upstream_id.is_none() && self.upstream.is_none() {
            Err(ValidationError::new("upstream_required"))
        } else {
            Ok(())
        }
    }
}
/// Global rules apply plugins to all matching mcp requests
#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct GlobalRule {
    /// The id of the global rule.
    #[serde(default)]
    pub id: String,
    /// The plugins of the global rule.
    /// The key is the plugin name, the value is the plugin configuration.
    #[serde(default)]
    pub plugins: HashMap<String, YamlValue>,
}

/// SSL configuration for the mcp server.
#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct SSL {
    #[serde(default)]
    /// The id of the SSL configuration.
    pub id: String,
    /// The certificate of the SSL configuration.
    pub cert: String,
    /// The key of the SSL configuration.
    pub key: String,
    /// The SNIs of the SSL configuration.
    #[validate(length(min = 1))]
    pub snis: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_log() {
        let _ = env_logger::builder().is_test(true).try_init();
    }

    #[test]
    fn not_a_test_i_cannot_write_yaml_by_hand() {
        init_log();
        let conf = Config::default();
        // cargo test -- --nocapture not_a_test_i_cannot_write_yaml_by_hand
        println!("{}", conf.to_yaml());
    }

    #[test]
    fn test_load_file() {
        init_log();
        let conf_str = r#"
---
pingora:
  version: 1
  client_bind_to_ipv4:
      - 1.2.3.4
      - 5.6.7.8
  client_bind_to_ipv6: []

access_point:
  listeners:
    - address: 0.0.0.0:8080
    - address: "[::1]:8080"
      tls:
        cert_path: /etc/ssl/server.crt
        key_path: /etc/ssl/server.key
      offer_h2: true

routes:
  - id: 1
    uri: /
    upstream:
      nodes:
        "127.0.0.1:1980": 1
      checks:
        active:
          type: http

upstreams:
  - nodes:
      "127.0.0.1:1980": 1
    id: 1
    checks:
      active:
        type: http

services:
  - id: 1
    upstream_id: 1
    hosts: ["example.com"]
        "#
        .to_string();
        let conf = Config::from_yaml(&conf_str).unwrap();
        assert_eq!(2, conf.pingora.client_bind_to_ipv4.len());
        assert_eq!(0, conf.pingora.client_bind_to_ipv6.len());
        assert_eq!(1, conf.pingora.version);
        assert_eq!(2, conf.access_point.listeners.len());
        assert_eq!(1, conf.routes.len());
        assert_eq!(1, conf.upstreams.len());
        assert_eq!(1, conf.services.len());
        print!("{}", conf.to_yaml());
    }

    #[test]
    fn test_load_file_upstream_id() {
        init_log();
        let conf_str = r#"
---
pingora:
  version: 1
  client_bind_to_ipv4:
      - 1.2.3.4
      - 5.6.7.8
  client_bind_to_ipv6: []

access_point:
  listeners:
    - address: 0.0.0.0:8080
      offer_h2c: true
    - address: "[::1]:8080"
      tls:
        cert_path: /etc/ssl/server.crt
        key_path: /etc/ssl/server.key
      offer_h2: true

routes:
  - id: 1
    uri: /
    upstream_id: 1

upstreams:
  - nodes:
      "127.0.0.1:1980": 1
    id: 1
    checks:
      active:
        type: http
  - nodes:
      "127.0.0.1:1981": 1
    id: 2
    checks:
      active:
        type: http

services:
  - id: 1
    upstream_id: 1
    hosts: ["example.com"]
        "#
        .to_string();
        let conf = Config::from_yaml(&conf_str).unwrap();
        assert_eq!(2, conf.pingora.client_bind_to_ipv4.len());
        assert_eq!(0, conf.pingora.client_bind_to_ipv6.len());
        assert_eq!(1, conf.pingora.version);
        assert_eq!(2, conf.access_point.listeners.len());
        assert_eq!(1, conf.routes.len());
        assert_eq!(2, conf.upstreams.len());
        assert_eq!(1, conf.services.len());
        print!("{}", conf.to_yaml());
    }

    #[test]
    fn test_valid_listeners_length() {
        init_log();
        let conf_str = r#"
---
access_point:
  listeners: []

routes:
  - id: 1
    uri: /
    upstream:
      nodes:
        "127.0.0.1:1980": 1
        "#
        .to_string();
        let conf = Config::from_yaml(&conf_str);
        // Check for error and print the result
        match conf {
            Ok(_) => panic!("Expected error, but got a valid config"),
            Err(e) => {
                // Print the error here
                eprintln!("Error: {:?}", e);
                assert!(true); // You can assert true because you expect an error
            }
        }
    }

    #[test]
    fn test_valid_listeners_tls_for_offer_h2() {
        init_log();
        let conf_str = r#"
---
access_point:
  listeners:
    - address: "[::1]:8080"
      offer_h2: true

routes:
  - id: 1
    uri: /
    upstream:
      nodes:
        "127.0.0.1:1980": 1
        "#
        .to_string();
        let conf = Config::from_yaml(&conf_str);
        // Check for error and print the result
        match conf {
            Ok(_) => panic!("Expected error, but got a valid config"),
            Err(e) => {
                // Print the error here
                eprintln!("Error: {:?}", e);
                assert!(true); // You can assert true because you expect an error
            }
        }
    }

    #[test]
    fn test_valid_routes_uri_and_uris() {
        init_log();
        let conf_str = r#"
---
access_point:
  listeners:
    - address: "[::1]:8080"

routes:
  - id: 1
    upstream:
      nodes:
        "127.0.0.1:1980": 1
        "#
        .to_string();
        let conf = Config::from_yaml(&conf_str);
        // Check for error and print the result
        match conf {
            Ok(_) => panic!("Expected error, but got a valid config"),
            Err(e) => {
                // Print the error here
                eprintln!("Error: {:?}", e);
                assert!(true); // You can assert true because you expect an error
            }
        }
    }

    #[test]
    fn test_valid_routes_upstream_host() {
        init_log();
        let conf_str = r#"
---
access_point:
  listeners:
    - address: "[::1]:8080"

routes:
  - id: 1
    upstream:
      nodes:
        "127.0.0.1:1980": 1
      pass_host: rewrite
        "#
        .to_string();
        let conf = Config::from_yaml(&conf_str);
        // Check for error and print the result
        match conf {
            Ok(_) => panic!("Expected error, but got a valid config"),
            Err(e) => {
                // Print the error here
                eprintln!("Error: {:?}", e);
                assert!(true); // You can assert true because you expect an error
            }
        }
    }

    #[test]
    fn test_valid_config_upstream_id() {
        init_log();
        let conf_str = r#"
---
access_point:
  listeners:
    - address: "[::1]:8080"

routes:
  - id: 1
    uri: /
    upstream:
      nodes:
        "127.0.0.1:1980": 1
      checks:
        active:
          type: http

upstreams:
  - nodes:
      "127.0.0.1:1980": 1
    checks:
      active:
        type: http
        "#
        .to_string();
        let conf = Config::from_yaml(&conf_str);
        // Check for error and print the result
        match conf {
            Ok(_) => panic!("Expected error, but got a valid config"),
            Err(e) => {
                // Print the error here
                eprintln!("Error: {:?}", e);
                assert!(true); // You can assert true because you expect an error
            }
        }
    }

    #[test]
    fn test_valid_route_upstream() {
        init_log();
        let conf_str = r#"
---
access_point:
  listeners:
    - address: "[::1]:8080"

routes:
  - id: 1
    uri: /
        "#
        .to_string();
        let conf = Config::from_yaml(&conf_str);
        // Check for error and print the result
        match conf {
            Ok(_) => panic!("Expected error, but got a valid config"),
            Err(e) => {
                // Print the error here
                eprintln!("Error: {:?}", e);
                assert!(true); // You can assert true because you expect an error
            }
        }
    }

    #[test]
    fn test_valid_service_upstream() {
        init_log();
        let conf_str = r#"
---
access_point:
  listeners:
    - address: "[::1]:8080"

routes:
  - id: 1
    uri: /
    upstream:
      nodes:
        "127.0.0.1:1980": 1
      checks:
        active:
          type: http

services:
  - id: 1
    hosts: ["example.com"]
        "#
        .to_string();
        let conf = Config::from_yaml(&conf_str);
        // Check for error and print the result
        match conf {
            Ok(_) => panic!("Expected error, but got a valid config"),
            Err(e) => {
                // Print the error here
                eprintln!("Error: {:?}", e);
                assert!(true); // You can assert true because you expect an error
            }
        }
    }
}
