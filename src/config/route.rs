use std::{collections::HashMap, fmt};

use pingora_error::Result;
use serde::{Deserialize, Serialize};
use serde_yaml::Value as YamlValue;
use validator::{Validate, ValidationError};

use super::{Timeout, Upstream};

/// HTTP Methods.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HttpMethod {
    /// GET method.
    GET,
    /// POST method.
    POST,
    /// PUT method.
    PUT,
    /// DELETE method.
    DELETE,
    /// PATCH method.
    PATCH,
    /// HEAD method.
    HEAD,
    /// OPTIONS method.
    OPTIONS,
    /// CONNECT method.
    CONNECT,
    /// TRACE method.
    TRACE,
    // PURGE,
}
impl HttpMethod {
    /// Convert to http::Method
    pub fn to_http_method(&self) -> http::Method {
        match self {
            HttpMethod::GET => http::Method::GET,
            HttpMethod::POST => http::Method::POST,
            HttpMethod::PUT => http::Method::PUT,
            HttpMethod::DELETE => http::Method::DELETE,
            HttpMethod::PATCH => http::Method::PATCH,
            HttpMethod::HEAD => http::Method::HEAD,
            HttpMethod::OPTIONS => http::Method::OPTIONS,
            HttpMethod::CONNECT => http::Method::CONNECT,
            HttpMethod::TRACE => http::Method::TRACE,
            // HttpMethod::PURGE => http::Method::PURGE,
        }
    }
    /// Convert from http::Method
    pub fn from_http_method(method: &http::Method) -> Option<Self> {
        match method.as_str() {
            "GET" => Some(HttpMethod::GET),
            "POST" => Some(HttpMethod::POST),
            "PUT" => Some(HttpMethod::PUT),
            "DELETE" => Some(HttpMethod::DELETE),
            "PATCH" => Some(HttpMethod::PATCH),
            "HEAD" => Some(HttpMethod::HEAD),
            "OPTIONS" => Some(HttpMethod::OPTIONS),
            "CONNECT" => Some(HttpMethod::CONNECT),
            "TRACE" => Some(HttpMethod::TRACE),
            // "PURGE" => Some(HttpMethod::PURGE),
            _ => None,
        }
    }
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let method = match self {
            HttpMethod::GET => "GET",
            HttpMethod::POST => "POST",
            HttpMethod::PUT => "PUT",
            HttpMethod::DELETE => "DELETE",
            HttpMethod::PATCH => "PATCH",
            HttpMethod::HEAD => "HEAD",
            HttpMethod::OPTIONS => "OPTIONS",
            HttpMethod::CONNECT => "CONNECT",
            HttpMethod::TRACE => "TRACE",
            // HttpMethod::PURGE => "PURGE",
        };
        write!(f, "{}", method)
    }
}

/// Route configuration for a single route
#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[validate(schema(function = "Route::validate"))]
pub struct Route {
    /// The unique ID of the route.
    /// This ID is used to identify the route in the configuration.
    #[serde(default)]
    pub id: String,
    /// The URI pattern to match requests against.
    /// This can be a single URI or a list of URIs.
    /// If both `uri` and `uris` are specified, `uris` will be used.
    /// If neither `uri` nor `uris` are specified, an error will be returned.
    pub uri: Option<String>,
    /// A list of URIs to match requests against.
    /// This can be used to specify multiple URIs for the same route.
    /// If both `uri` and `uris` are specified, `uris` will be used.
    #[serde(default)]
    pub uris: Vec<String>,
    /// The HTTP methods to match requests against.
    #[serde(default)]
    pub methods: Vec<HttpMethod>,
    /// The host to match requests against.
    pub host: Option<String>,
    /// A list of hosts to match requests against.
    /// This can be used to specify multiple hosts for the same route.
    /// If both `host` and `hosts` are specified, `hosts` will be used.
    #[serde(default)]
    pub hosts: Vec<String>,
    /// The priority of the route.
    /// This is used to determine the order in which routes are matched.
    /// Routes with a higher priority will be matched before routes with a lower priority.
    /// If no priority is specified, the default priority is 0.
    #[serde(default = "Route::default_priority")]
    pub priority: u32,
    /// The plugins to be applied to the route.
    /// This is a map of plugin names to plugin configurations.
    /// The plugin configurations are specified as YAML values.
    /// If no plugins are specified, an empty map will be used.
    #[serde(default)]
    pub plugins: HashMap<String, YamlValue>,
    /// The upstream to be used by the route.
    /// If no upstream is specified, must configuthe `upstream_id` or `service_id`.
    /// If both `upstream` and `upstream_id` are specified, `upstream` will be used.
    /// If both `upstream` and `service_id` are specified, `upstream` will be used.
    /// If neither `upstream` nor `upstream_id` are specified, an error will be returned.
    #[validate(nested)]
    pub upstream: Option<Upstream>,
    /// The ID of the upstream to be used by the route.
    pub upstream_id: Option<String>,
    /// The ID of the service to be used by the route.
    pub service_id: Option<String>,
    /// The timeout settings for the route.
    #[validate(nested)]
    pub timeout: Option<Timeout>,
}

impl Route {
    fn validate(&self) -> Result<(), ValidationError> {
        if self.uri.is_none() && self.uris.is_empty() {
            return Err(ValidationError::new("uri_or_uris_required"));
        }

        if self.upstream_id.is_none() && self.service_id.is_none() && self.upstream.is_none() {
            return Err(ValidationError::new("upstream_or_service_required"));
        }

        Ok(())
    }
    /// Get hosts from host and hosts.
    pub fn get_hosts(&self) -> Vec<String> {
        self.host
            .clone()
            .map_or_else(|| self.hosts.clone(), |host| vec![host.to_string()])
    }
    /// Get uris from uri and uris.
    pub fn get_uris(&self) -> Vec<String> {
        self.uri
            .clone()
            .map_or_else(|| self.uris.clone(), |uri| vec![uri.to_string()])
    }

    fn default_priority() -> u32 {
        0
    }
}
