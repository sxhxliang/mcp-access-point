use async_trait::async_trait;
use http::Uri;
use pingora_error::{
    ErrorType::{InternalError, ReadError},
    OrErr, Result,
};
use pingora_http::RequestHeader;
use pingora_proxy::Session;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_yaml::Value as YamlValue;
use std::sync::Arc;
use validator::{Validate, ValidationError};

use super::{apply_regex_uri_template, ProxyPlugin};
use crate::proxy::ProxyContext;

pub const PLUGIN_NAME: &str = "proxy-rewrite";
const PRIORITY: i32 = 1008;

pub fn create_proxy_rewrite_plugin(cfg: YamlValue) -> Result<Arc<dyn ProxyPlugin>> {
    let config: PluginConfig =
        serde_yaml::from_value(cfg).or_err(ReadError, "Invalid proxy rewrite plugin config")?;

    config
        .validate()
        .or_err(ReadError, "Invalid proxy rewrite plugin config")?;

    // Precompile regex patterns for regex_uri to improve performance
    let mut regex_patterns = Vec::new();
    for i in (0..config.regex_uri.len()).step_by(2) {
        let pattern = &config.regex_uri[i];
        let template = &config.regex_uri[i + 1];
        // Validation ensures regex is valid, so unwrap is safe
        let re = Regex::new(pattern).unwrap();
        regex_patterns.push((re, template.clone()));
    }

    Ok(Arc::new(PluginProxyRewrite {
        config,
        regex_patterns,
    }))
}

#[derive(Clone, Default, Debug, Serialize, Deserialize, Validate)]
struct Head {
    name: String,
    value: String,
}

#[derive(Clone, Default, Debug, Serialize, Deserialize, Validate)]
struct Headers {
    #[serde(default)]
    add: Vec<Head>,
    #[serde(default)]
    set: Vec<Head>,
    #[serde(default)]
    remove: Vec<String>,
}

#[derive(Default, Debug, Serialize, Deserialize, Validate)]
struct PluginConfig {
    /// The URI to rewrite to. Takes precedence over `regex_uri` if both are set.
    uri: Option<String>,
    method: Option<String>,
    #[serde(default)]
    #[validate(custom(function = "PluginConfig::validate_regex_uri"))]
    regex_uri: Vec<String>,
    host: Option<String>,
    headers: Option<Headers>,
}

impl PluginConfig {
    fn validate_regex_uri(regex_uri: &[String]) -> Result<(), ValidationError> {
        if regex_uri.len() % 2 != 0 {
            return Err(ValidationError::new("regex_uri_length"));
        }

        regex_uri
            .iter()
            .enumerate()
            .filter(|(i, _)| i % 2 == 0)
            .map(|(_, pattern)| {
                Regex::new(pattern).map_err(|_| ValidationError::new("invalid_regex"))
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(())
    }
}

pub struct PluginProxyRewrite {
    config: PluginConfig,
    regex_patterns: Vec<(Regex, String)>, // Precompiled regex and template pairs
}

#[async_trait]
impl ProxyPlugin for PluginProxyRewrite {
    fn name(&self) -> &str {
        PLUGIN_NAME
    }

    fn priority(&self) -> i32 {
        PRIORITY
    }

    async fn upstream_request_filter(
        &self,
        session: &mut Session,
        upstream_request: &mut RequestHeader,
        _ctx: &mut ProxyContext,
    ) -> Result<()> {
        if let Some(path_and_query) = session.req_header().uri.path_and_query() {
            if let Some(uri) = self.construct_path_and_query(Some(path_and_query)) {
                upstream_request.set_uri(uri);
            }
        }

        if let Some(ref method) = self.config.method {
            upstream_request.set_method(
                method
                    .as_bytes()
                    .try_into()
                    .or_err(InternalError, "Invalid method")?,
            );
        }

        if let Some(ref host) = self.config.host {
            upstream_request
                .insert_header(http::header::HOST, host)
                .or_err(InternalError, "Invalid host")?;
        }

        if let Some(ref headers) = self.config.headers {
            self.apply_headers(upstream_request, headers)?;
        }

        Ok(())
    }
}

impl PluginProxyRewrite {
    fn construct_path_and_query(
        &self,
        path_and_query: Option<&http::uri::PathAndQuery>,
    ) -> Option<Uri> {
        if let Some(ref path) = self.config.uri {
            let query = path_and_query.and_then(|pq| pq.query()).unwrap_or("");
            return if query.is_empty() {
                path.parse().ok()
            } else {
                format!("{}?{}", path, query).parse().ok()
            };
        }

        if !self.regex_patterns.is_empty() {
            if let Some(pq) = path_and_query {
                let query = pq.query().unwrap_or("");
                let new_path = apply_regex_uri_template(pq.path(), &self.regex_patterns);
                return if query.is_empty() {
                    new_path.parse().ok()
                } else {
                    format!("{}?{}", new_path, query).parse().ok()
                };
            }
        }

        None
    }

    fn apply_headers(&self, upstream_request: &mut RequestHeader, headers: &Headers) -> Result<()> {
        for head in &headers.set {
            upstream_request.insert_header(head.name.clone(), &head.value)?;
        }

        for name in &headers.remove {
            upstream_request.remove_header(name);
        }

        for head in &headers.add {
            upstream_request.append_header(head.name.clone(), &head.value)?;
        }

        Ok(())
    }
}