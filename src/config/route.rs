use std::{collections::HashMap, fmt};

use pingora_error::Result;
use serde::{Deserialize, Serialize};
use serde_yaml::Value as YamlValue;
use validator::{Validate, ValidationError};

use super::{Timeout, Upstream};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HttpMethod {
    GET,
    POST,
    PUT,
    DELETE,
    PATCH,
    HEAD,
    OPTIONS,
    CONNECT,
    TRACE,
    // PURGE,
}
impl HttpMethod {
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

#[derive(Clone, Default, Debug, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[validate(schema(function = "Route::validate"))]
pub struct Route {
    #[serde(default)]
    pub id: String,

    pub uri: Option<String>,
    #[serde(default)]
    pub uris: Vec<String>,
    #[serde(default)]
    pub methods: Vec<HttpMethod>,
    pub host: Option<String>,
    #[serde(default)]
    pub hosts: Vec<String>,
    #[serde(default = "Route::default_priority")]
    pub priority: u32,

    #[serde(default)]
    pub plugins: HashMap<String, YamlValue>,
    #[validate(nested)]
    pub upstream: Option<Upstream>,
    pub upstream_id: Option<String>,
    pub service_id: Option<String>,
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

    pub fn get_hosts(&self) -> Vec<String> {
        self.host
            .clone()
            .map_or_else(|| self.hosts.clone(), |host| vec![host.to_string()])
    }

    pub fn get_uris(&self) -> Vec<String> {
        self.uri
            .clone()
            .map_or_else(|| self.uris.clone(), |uri| vec![uri.to_string()])
    }

    fn default_priority() -> u32 {
        0
    }
}
