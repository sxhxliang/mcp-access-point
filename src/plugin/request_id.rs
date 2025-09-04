use std::sync::Arc;

use async_trait::async_trait;
use pingora::ErrorType::InternalError;
use pingora_error::{ErrorType::ReadError, OrErr, Result};
use pingora_http::ResponseHeader;
use pingora_proxy::Session;
use rand::prelude::*;
use serde::{Deserialize, Serialize};
use serde_yaml::Value as YamlValue;
use uuid::Uuid;
use validator::{Validate, ValidationError};

use crate::{proxy::ProxyContext, utils::request};

use super::ProxyPlugin;

pub const PLUGIN_NAME: &str = "request-id";
const DEFAULT_CHAR_SET: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIGKLMNOPQRSTUVWXYZ0123456789";
/// Creates an Key Auth plugin instance with the given configuration.
pub fn create_request_id_plugin(cfg: YamlValue) -> Result<Arc<dyn ProxyPlugin>> {
    let config: PluginConfig = serde_yaml::from_value(cfg)
        .or_err_with(ReadError, || "Invalid request id plugin config")?;

    config
        .validate()
        .or_err_with(ReadError, || "Invalid request id plugin config")?;

    Ok(Arc::new(PluginRequestID { config }))
}

/// Configuration for the Request ID plugin.
#[derive(Default, Debug, Serialize, Deserialize, Validate)]
struct PluginConfig {
    #[serde(default = "PluginConfig::default_header_name")]
    header_name: String,
    #[serde(default = "PluginConfig::default_include_in_response")]
    include_in_response: bool,
    #[serde(default = "PluginConfig::default_algorithm")]
    #[validate(custom(function = "PluginConfig::validate_algorithm"))]
    algorithm: String,
    #[serde(default)]
    range_id: RangeID,
}

impl PluginConfig {
    fn default_header_name() -> String {
        "X-Request-Id".to_string()
    }

    fn default_include_in_response() -> bool {
        true
    }

    fn default_algorithm() -> String {
        "uuid".to_string()
    }

    fn validate_algorithm(algorithm: &String) -> Result<(), ValidationError> {
        if algorithm == "uuid" || algorithm == "range_id" {
            Ok(())
        } else {
            Err(ValidationError::new(
                "algorithm must be either 'uuid' or 'range_id'",
            ))
        }
    }
}

#[derive(Default, Debug, Serialize, Deserialize)]
struct RangeID {
    #[serde(default = "RangeID::default_char_set")]
    char_set: String,
    #[serde(default = "RangeID::default_length")]
    length: u32,
}

impl RangeID {
    pub fn default_char_set() -> String {
        "abcdefghijklmnopqrstuvwxyzABCDEFGHIGKLMNOPQRSTUVWXYZ0123456789".to_string()
    }

    pub fn default_length() -> u32 {
        16
    }
}

pub struct PluginRequestID {
    config: PluginConfig,
}

impl PluginRequestID {
    fn get_request_id(&self) -> String {
        match self.config.algorithm.as_str() {
            "uuid" => Uuid::new_v4().to_string(),
            "range_id" => self.get_range_id(),
            _ => Uuid::new_v4().to_string(),
        }
    }

    fn get_range_id(&self) -> String {
        let char_set = if self.config.range_id.char_set.is_empty() {
            DEFAULT_CHAR_SET
        } else {
            &self.config.range_id.char_set
        };
        let chars: Vec<char> = char_set.chars().collect();
        let mut rng = rand::rng();
        (0..self.config.range_id.length)
            .map(|_| *chars.choose(&mut rng).unwrap())
            .collect()
    }
}

#[async_trait]
impl ProxyPlugin for PluginRequestID {
    fn name(&self) -> &str {
        PLUGIN_NAME
    }

    fn priority(&self) -> i32 {
        12015
    }

    async fn request_filter(&self, session: &mut Session, ctx: &mut ProxyContext) -> Result<bool> {
        // 从头里拿request id，如果没有生成一个，写入请求头
        let value =
            match request::get_req_header_value(session.req_header(), &self.config.header_name) {
                Some(s) => s.to_string(),
                None => {
                    let request_id = self.get_request_id();
                    session
                        .req_header_mut()
                        .insert_header(self.config.header_name.clone(), &request_id)
                        .or_err_with(InternalError, || "Session insert header fail")?;
                    request_id
                }
            };

        ctx.vars.insert("request-id".to_string(), value);

        Ok(false)
    }

    async fn response_filter(
        &self,
        _session: &mut Session,
        upstream_response: &mut ResponseHeader,
        ctx: &mut ProxyContext,
    ) -> Result<()> {
        if self.config.include_in_response {
            let value = ctx.vars.get("request-id").unwrap();

            upstream_response
                .insert_header(self.config.header_name.clone(), value)
                .or_err_with(InternalError, || "Upstream response insert header fail")?;
        }

        Ok(())
    }
}
