// pub mod etcd;

use std::collections::HashMap;

use pingora_error::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

use super::Timeout;
/// Upstream represents a backend service that can be used by a route.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[validate(schema(function = "Upstream::validate_upstream_host"))]
pub struct Upstream {
    #[serde(default)]
    /// Unique identifier for the upstream.
    pub id: String,
    /// `retries` is the number of retries to attempt before failing.
    pub retries: Option<u32>,
    /// `retry_timeout` is the timeout for each retry attempt.
    pub retry_timeout: Option<u64>,
    /// `timeout` is the timeout for each attempt.
    #[validate(nested)]
    pub timeout: Option<Timeout>,
    /// nodes is a list of backend service nodes.
    /// The key is the backend service address, and the value is the weight of the node.
    /// Each node must have an address and a port.
    /// The address can be an IP address or a domain name.
    #[validate(custom(function = "Upstream::validate_nodes_keys"))]
    #[serde(default)]
    pub nodes: HashMap<String, u32>, // backend service address
    /// `type` is the loadbalancer type.
    /// contains: RoundRobin, Random, Fnv, Ketama.
    /// Default is RoundRobin.
    #[serde(default)]
    pub r#type: SelectionType,
    /// `checks` is the health check configuration.
    #[validate(nested)]
    pub checks: Option<HealthCheck>,
    /// `hash_on` is the hash key for the upstream.
    #[serde(default)]
    pub hash_on: UpstreamHashOn,
    /// `key` is the key to hash on.
    #[serde(default = "Upstream::default_key")]
    pub key: String,
    /// `scheme` is the scheme to use for the upstream.
    /// contains: HTTP, HTTPS. default is HTTP.
    #[serde(default)]
    pub scheme: UpstreamScheme,
    /// `pass_host` is the pass host configuration.
    /// contains: REWRITE, KEEP. default is KEEP.
    #[serde(default)]
    pub pass_host: UpstreamPassHost,
    /// `upstream_host` is the upstream host to use.
    pub upstream_host: Option<String>,
    /// `headers` is the headers to use for the upstream request.
    #[serde(default)]
    pub headers: Option<HashMap<String, String>>,
}

impl Default for Upstream {
    fn default() -> Self {
        Self {
            id: String::new(),
            retries: None,
            retry_timeout: None,
            timeout: None,
            nodes: HashMap::new(),
            r#type: SelectionType::default(),
            checks: None,
            hash_on: UpstreamHashOn::default(),
            key: Upstream::default_key(),
            scheme: UpstreamScheme::default(),
            pass_host: UpstreamPassHost::default(),
            upstream_host: None,
            headers: None,
        }
    }
}

impl Upstream {
    /// 合并两个Upstream对象
    pub fn merge(&mut self, other: Upstream) {
        // 合并简单字段
        if !other.id.is_empty() {
            self.id = other.id;
        }
        if other.retries.is_some() {
            self.retries = other.retries;
        }
        if other.retry_timeout.is_some() {
            self.retry_timeout = other.retry_timeout;
        }
        if other.timeout.is_some() {
            self.timeout = other.timeout;
        }
        if other.r#type != SelectionType::default() {
            self.r#type = other.r#type;
        }
        if other.hash_on != UpstreamHashOn::default() {
            self.hash_on = other.hash_on;
        }
        if other.key != Self::default_key() {
            self.key = other.key;
        }
        if other.scheme != UpstreamScheme::default() {
            self.scheme = other.scheme;
        }
        if other.pass_host != UpstreamPassHost::default() {
            self.pass_host = other.pass_host;
        }
        if other.upstream_host.is_some() {
            self.upstream_host = other.upstream_host;
        }

        // 合并nodes
        for (k, v) in other.nodes {
            self.nodes.insert(k, v);
        }

        // 合并headers
        if let Some(other_headers) = other.headers {
            if let Some(headers) = &mut self.headers {
                for (k, v) in other_headers {
                    headers.insert(k, v);
                }
            } else {
                self.headers = Some(other_headers);
            }
        }

        // 合并checks
        if other.checks.is_some() {
            self.checks = other.checks;
        }
    }

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

/// loadbalancer type for upstream.
/// contains: RoundRobin, Random, Fnv, Ketama.
/// Default is RoundRobin.
#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SelectionType {
    /// RoundRobin is the round-robin loadbalancer.
    #[default]
    RoundRobin,
    /// Random is the random loadbalancer.
    Random,
    /// Fnv is the fnv loadbalancer.
    Fnv,
    /// Ketama is the ketama loadbalancer.
    Ketama,
}

/// health check configuration for upstream.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct HealthCheck {
    /// only support passive check for now
    #[validate(nested)]
    pub active: ActiveCheck,
}

/// active check configuration for upstream.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct ActiveCheck {
    /// `type` is the active check type.
    /// contains: TCP, HTTP, HTTPS. default is HTTP.
    #[serde(default)]
    pub r#type: ActiveCheckType,
    /// `timeout` is the timeout for the active check. default is 1.
    #[serde(default = "ActiveCheck::default_timeout")]
    pub timeout: u32,
    /// `http_path` is the path to use for the active check. default is "/".
    #[serde(default = "ActiveCheck::default_http_path")]
    pub http_path: String,
    /// `host` is the host to use for the active check. default is the upstream host.
    pub host: Option<String>,
    /// `port` is the port to use for the active check. default is the upstream port.
    pub port: Option<u32>,
    /// `https_verify_certificate` is the https verify certificate. default is true.
    #[serde(default = "ActiveCheck::default_https_verify_certificate")]
    pub https_verify_certificate: bool,
    #[serde(default)]
    /// `req_headers` is the headers to use for the active check request.
    pub req_headers: Vec<String>,
    /// `healthy` is the healthy configuration for the active check.
    pub healthy: Option<Health>,
    /// `unhealthy` is the unhealthy configuration for the active check.
    #[validate(nested)]
    pub unhealthy: Option<Unhealthy>,
}

/// active check type.
#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActiveCheckType {
    /// TCP is the tcp active check type.
    TCP,
    /// HTTP is the http active check type.
    #[default]
    HTTP,
    /// HTTPS is the https active check type.
    HTTPS,
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
/// Health represents the health check configuration for an upstream.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Health {
    /// `interval` is the interval for the health check. default is 1.
    #[serde(default = "Health::default_interval")]
    pub interval: u32,
    /// `http_statuses` is the http statuses to use for the health check. default is [200, 302].
    #[serde(default = "Health::default_http_statuses")]
    pub http_statuses: Vec<u32>,
    /// `successes` is the number of successes to consider the upstream healthy. default is 2.
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

/// Unhealthy represents the unhealthy check configuration for an upstream.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct Unhealthy {
    /// `http_failures` is the number of http failures to consider the upstream unhealthy. default is 5.
    #[serde(default = "Unhealthy::default_http_failures")]
    pub http_failures: u32,
    /// `tcp_failures` is the number of tcp failures to consider the upstream unhealthy. default is 2.
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
/// UpstreamHashOn represents the hash on configuration for an upstream.
#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpstreamHashOn {
    /// VARS is the vars hash on.
    #[default]
    VARS,
    /// HEAD is the head hash on.
    HEAD,
    /// COOKIE is the cookie hash on.
    COOKIE,
}

/// UpstreamScheme represents the scheme configuration for an upstream.
/// protocol contains: HTTP, HTTPS. default is HTTP.
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpstreamScheme {
    /// HTTP is the http protocol.
    #[default]
    HTTP,
    /// HTTPS is the https protocol.
    HTTPS,
    /// GRPC is the grpc protocol.
    GRPC,
    /// GRPCS is the grpcs protocol.
    GRPCS,
}

/// UpstreamPassHost represents the pass host configuration for an upstream.
/// contains: REWRITE, KEEP. default is KEEP.
#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpstreamPassHost {
    /// KEEP is the keep pass host.
    #[default]
    PASS,
    /// REWRITE is the rewrite pass host.
    REWRITE,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_all_fields_set() {
        let mut base = Upstream {
            id: "base_id".to_string(),
            retries: Some(1),
            retry_timeout: Some(1000),
            timeout: Some(Timeout {
                connect: 3000,
                send: 12,
                read: 12,
            }),
            r#type: SelectionType::default(),
            hash_on: UpstreamHashOn::default(),
            key: Upstream::default_key(),
            scheme: UpstreamScheme::default(),
            pass_host: UpstreamPassHost::default(),
            upstream_host: None,
            nodes: HashMap::new(),
            headers: None,
            checks: None,
        };

        let other = Upstream {
            id: "new_id".to_string(),
            retries: Some(3),
            retry_timeout: Some(2000),
            timeout: Some(Timeout {
                connect: 6000,
                send: 12,
                read: 12,
            }),
            r#type: SelectionType::RoundRobin,
            hash_on: UpstreamHashOn::HEAD,
            key: "custom_key".to_string(),
            scheme: UpstreamScheme::HTTPS,
            pass_host: UpstreamPassHost::REWRITE,
            upstream_host: Some("host.com".to_string()),
            nodes: {
                let mut map = HashMap::new();
                map.insert("node1".to_string(), 1);
                map
            },
            headers: {
                let mut map = HashMap::new();
                map.insert("header1".to_string(), "value1".to_string());
                Some(map)
            },
            checks: None,
        };

        base.merge(other);

        assert_eq!(base.id, "new_id");
        assert_eq!(base.retries, Some(3));
        assert_eq!(base.retry_timeout, Some(2000));
        assert_eq!(
            base.timeout,
            Some(Timeout {
                connect: 6000,
                send: 12,
                read: 12
            })
        );
        assert_eq!(base.r#type, SelectionType::RoundRobin);
        assert_eq!(base.hash_on, UpstreamHashOn::HEAD);
        assert_eq!(base.key, "custom_key");
        assert_eq!(base.scheme, UpstreamScheme::HTTPS);
        assert_eq!(base.pass_host, UpstreamPassHost::REWRITE);
        assert_eq!(base.upstream_host.unwrap(), "host.com");
        assert_eq!(base.nodes.len(), 1);
        assert_eq!(base.headers.as_ref().unwrap().len(), 1);
        assert_eq!(base.checks, None);
    }
}
