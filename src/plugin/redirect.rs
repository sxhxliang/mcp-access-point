use std::sync::Arc;

use async_trait::async_trait;
use http::{header, uri::Scheme, StatusCode, Uri};
use pingora_error::{ErrorType::ReadError, OrErr, Result};
use pingora_http::ResponseHeader;
use pingora_proxy::Session;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_yaml::Value as YamlValue;
use validator::{Validate, ValidationError};

use crate::{proxy::ProxyContext, utils::request::get_request_host};

use super::{apply_regex_uri_template, ProxyPlugin};

pub const PLUGIN_NAME: &str = "redirect";
const PRIORITY: i32 = 900;

pub fn create_redirect_plugin(cfg: YamlValue) -> Result<Arc<dyn ProxyPlugin>> {
    let config: PluginConfig =
        serde_yaml::from_value(cfg).or_err(ReadError, "Invalid redirect plugin config")?;

    config
        .validate()
        .or_err(ReadError, "Invalid redirect plugin config")?;

    // Precompile regex patterns for regex_uri to improve performance
    let mut regex_patterns = Vec::new();
    for i in (0..config.regex_uri.len()).step_by(2) {
        let pattern = &config.regex_uri[i];
        let template = &config.regex_uri[i + 1];
        // Validation ensures regex is valid, so unwrap is safe
        let re = Regex::new(pattern).unwrap();
        regex_patterns.push((re, template.clone()));
    }

    Ok(Arc::new(PluginRedirect {
        config,
        regex_patterns,
    }))
}

#[derive(Default, Debug, Serialize, Deserialize, Validate)]
struct PluginConfig {
    /// If true, redirects HTTP requests to HTTPS. Takes precedence over `uri` and `regex_uri`.
    #[serde(default)]
    http_to_https: bool,
    /// The URI to redirect to. Takes precedence over `regex_uri` if both are set.
    uri: Option<String>,
    /// List of regex pattern and replacement template pairs for URI rewriting.
    #[validate(custom(function = "PluginConfig::validate_regex_uri"))]
    regex_uri: Vec<String>,
    /// HTTP status code for the redirect (e.g., 301, 302, 307, 308). Defaults to 302 (temporary redirect).
    #[serde(default = "PluginConfig::default_ret_code")]
    ret_code: u16,
    /// If true, appends the original query string to the redirect URI, even if the target URI has a query string.
    #[serde(default)]
    append_query_string: bool,
}

impl PluginConfig {
    fn default_ret_code() -> u16 {
        302 // Default to temporary redirect (FOUND)
    }

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

pub struct PluginRedirect {
    config: PluginConfig,
    regex_patterns: Vec<(Regex, String)>, // Precompiled regex and template pairs
}

#[async_trait]
impl ProxyPlugin for PluginRedirect {
    fn name(&self) -> &str {
        PLUGIN_NAME
    }

    fn priority(&self) -> i32 {
        PRIORITY
    }

    async fn request_filter(&self, session: &mut Session, _ctx: &mut ProxyContext) -> Result<bool> {
        if self.config.http_to_https && session.req_header().uri.scheme() == Some(&Scheme::HTTP) {
            return self.redirect_https(session).await;
        }

        if let Some(new_uri) = self.construct_uri(session).await {
            return self.send_redirect_response(session, new_uri).await;
        }

        Ok(false)
    }
}

impl PluginRedirect {
    async fn redirect_https(&self, session: &mut Session) -> Result<bool> {
        let current_uri = session.req_header().uri.clone();
        let host = get_request_host(session.req_header())
            .ok_or_else(|| pingora_error::Error::new_str("Missing host"))?;

        let new_uri = Uri::builder()
            .scheme(Scheme::HTTPS)
            .authority(host)
            .path_and_query(current_uri.path_and_query().unwrap().to_owned())
            .build()
            .or_err(ReadError, "Failed to build HTTPS URI")?;

        self.send_redirect_response(session, new_uri).await
    }

    async fn construct_uri(&self, session: &mut Session) -> Option<Uri> {
        // Extract original query string directly from the URI to avoid borrowing parts
        let original_query = session
            .req_header()
            .uri
            .path_and_query()
            .and_then(|pq| pq.query())
            .unwrap_or("");
        let parts = session.req_header().uri.clone().into_parts();

        if let Some(ref path) = self.config.uri {
            return self.build_uri_from_path(path, parts, original_query);
        }

        if !self.regex_patterns.is_empty() {
            return self.build_uri_from_regex(parts, original_query);
        }

        None
    }

    fn build_uri_from_path(
        &self,
        path: &str,
        mut parts: http::uri::Parts,
        original_query: &str,
    ) -> Option<Uri> {
        let target_query = path
            .parse::<Uri>()
            .ok()
            .and_then(|uri| uri.query().map(|q| q.to_string()))
            .unwrap_or_default();
        let new_path = path.split('?').next().unwrap_or(path);
        let new_query = if self.config.append_query_string {
            if target_query.is_empty() {
                original_query.to_string()
            } else if original_query.is_empty() {
                target_query
            } else {
                format!("{}&{}", target_query, original_query)
            }
        } else {
            target_query
        };

        let new_path_and_query = if new_query.is_empty() {
            new_path.to_string()
        } else {
            format!("{}?{}", new_path, new_query)
        };

        parts.path_and_query = Some(new_path_and_query.parse().ok()?);
        Uri::from_parts(parts).ok()
    }

    fn build_uri_from_regex(
        &self,
        mut parts: http::uri::Parts,
        original_query: &str,
    ) -> Option<Uri> {
        if let Some(pq) = parts.path_and_query.take() {
            let path = pq.path();
            let target_query = pq.query().unwrap_or("");
            let new_path = apply_regex_uri_template(path, &self.regex_patterns);
            let new_query = if self.config.append_query_string {
                if target_query.is_empty() {
                    original_query.to_string()
                } else if original_query.is_empty() {
                    target_query.to_string()
                } else {
                    format!("{}&{}", target_query, original_query)
                }
            } else {
                target_query.to_string()
            };

            let new_uri = if new_query.is_empty() {
                new_path
            } else {
                format!("{}?{}", new_path, new_query)
            };
            parts.path_and_query = Some(new_uri.parse().ok()?);
            Uri::from_parts(parts).ok()
        } else {
            None
        }
    }

    async fn send_redirect_response(&self, session: &mut Session, new_uri: Uri) -> Result<bool> {
        let status_code = StatusCode::from_u16(self.config.ret_code).unwrap_or(StatusCode::FOUND); // Fallback to 302 if invalid
        let mut res_headers = ResponseHeader::build(status_code, Some(1))?;
        res_headers.append_header(header::LOCATION, new_uri.to_string())?;
        res_headers.append_header(header::CONTENT_TYPE, "text/plain")?;
        res_headers.append_header(header::CONTENT_LENGTH, 0)?;

        session
            .write_response_header(Box::new(res_headers), false)
            .await?;
        session
            .write_response_body(Some(bytes::Bytes::from_static(b"")), true)
            .await?;
        Ok(true)
    }
}