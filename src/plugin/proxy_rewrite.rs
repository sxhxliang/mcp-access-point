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

pub fn create_proxy_rewrite_plugin(cfg: YamlValue) -> Result<Arc<dyn ProxyPlugin>> {
    let config: PluginConfig = serde_yaml::from_value(cfg)
        .or_err_with(ReadError, || "Invalid proxy rewrite plugin config")?;

    config
        .validate()
        .or_err_with(ReadError, || "Invalid proxy rewrite plugin config")?;

    Ok(Arc::new(PluginProxyRewrite { config }))
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
}

#[async_trait]
impl ProxyPlugin for PluginProxyRewrite {
    fn name(&self) -> &str {
        PLUGIN_NAME
    }

    fn priority(&self) -> i32 {
        1008
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
                    .or_err_with(InternalError, || "Invalid method")?,
            );
        }

        if let Some(ref host) = self.config.host {
            upstream_request
                .insert_header(http::header::HOST, host)
                .or_err_with(InternalError, || "Invalid host")?;
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

        if !self.config.regex_uri.is_empty() {
            if let Some(pq) = path_and_query {
                let query = pq.query().unwrap_or("");
                let new_path = apply_regex_uri_template(
                    pq.path(),
                    &self
                        .config
                        .regex_uri
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<&str>>(),
                );
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
        headers.set.iter().for_each(|head| {
            upstream_request
                .insert_header(head.name.clone(), head.value.as_str())
                .or_err_with(InternalError, || "Invalid header")
                .unwrap();
        });

        headers.remove.iter().for_each(|name| {
            upstream_request.remove_header(name);
        });

        headers.add.iter().for_each(|head| {
            upstream_request
                .append_header(head.name.clone(), head.value.as_str())
                .or_err_with(InternalError, || "Invalid header")
                .unwrap();
        });

        Ok(())
    }
}
