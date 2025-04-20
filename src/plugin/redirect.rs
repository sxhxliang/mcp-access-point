use std::sync::Arc;

use async_trait::async_trait;
use http::{header, uri::Scheme, Uri};
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

pub fn create_redirect_plugin(cfg: YamlValue) -> Result<Arc<dyn ProxyPlugin>> {
    let config: PluginConfig =
        serde_yaml::from_value(cfg).or_err_with(ReadError, || "Invalid redirect plugin config")?;

    config
        .validate()
        .or_err_with(ReadError, || "Invalid redirect plugin config")?;

    Ok(Arc::new(PluginRedirect { config }))
}

#[derive(Default, Debug, Serialize, Deserialize, Validate)]
struct PluginConfig {
    #[serde(default)]
    http_to_https: bool,
    uri: Option<String>,
    #[validate(custom(function = "PluginConfig::validate_regex_uri"))]
    regex_uri: Vec<String>,
    #[serde(default = "PluginConfig::default_ret_code")]
    ret_code: u16,
    #[serde(default)]
    append_query_string: bool,
}

impl PluginConfig {
    fn default_ret_code() -> u16 {
        900
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
}

#[async_trait]
impl ProxyPlugin for PluginRedirect {
    fn name(&self) -> &str {
        PLUGIN_NAME
    }

    fn priority(&self) -> i32 {
        1008
    }

    async fn request_filter(&self, session: &mut Session, _ctx: &mut ProxyContext) -> Result<bool> {
        if self.config.http_to_https
            && session.req_header().uri.scheme() == Some(&Scheme::try_from("http").unwrap())
        {
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
        let host = get_request_host(session.req_header());

        if host.is_none() {
            return Ok(false);
        }

        let new_uri = Uri::builder()
            .scheme(Scheme::HTTPS)
            .authority(host.unwrap())
            .path_and_query(current_uri.path_and_query().unwrap().to_owned())
            .build()
            .unwrap();

        self.send_redirect_response(session, new_uri).await
    }

    async fn construct_uri(&self, session: &mut Session) -> Option<Uri> {
        let parts = session.req_header().uri.clone().into_parts();

        if let Some(ref path) = self.config.uri {
            return self.build_uri_from_path(path, parts);
        }

        if !self.config.regex_uri.is_empty() {
            return self.build_uri_from_regex(parts);
        }

        None
    }

    fn build_uri_from_path(&self, path: &str, mut parts: http::uri::Parts) -> Option<Uri> {
        let query = parts
            .path_and_query
            .as_ref()
            .and_then(|pq| pq.query())
            .unwrap_or("");
        let new_path = if query.is_empty() {
            path.to_string()
        } else {
            format!("{}?{}", path, query)
        };
        parts.path_and_query = Some(new_path.parse().ok()?);
        Uri::from_parts(parts).ok()
    }

    fn build_uri_from_regex(&self, mut parts: http::uri::Parts) -> Option<Uri> {
        if let Some(pq) = parts.path_and_query.take() {
            let path = pq.path();
            let query = pq.query().unwrap_or("");
            let new_path = apply_regex_uri_template(
                path,
                &self
                    .config
                    .regex_uri
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<&str>>(),
            );

            let new_uri = if query.is_empty() {
                new_path
            } else {
                format!("{}?{}", new_path, query)
            };
            parts.path_and_query = Some(new_uri.parse().ok()?);
            Some(Uri::from_parts(parts).ok()?)
        } else {
            None
        }
    }

    async fn send_redirect_response(&self, session: &mut Session, new_uri: Uri) -> Result<bool> {
        let mut res_headers = ResponseHeader::build_no_case(self.config.ret_code, Some(1))?;
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
