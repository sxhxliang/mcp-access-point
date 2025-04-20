use std::sync::Arc;

use async_trait::async_trait;
use http::{header, StatusCode};
use pingora_error::{ErrorType::ReadError, OrErr, Result};
use pingora_http::ResponseHeader;
use pingora_proxy::Session;
use serde::{Deserialize, Serialize};
use serde_yaml::Value as YamlValue;
use validator::Validate;

use crate::{proxy::ProxyContext, utils::request};

use super::ProxyPlugin;

pub const PLUGIN_NAME: &str = "key-auth";

/// Creates an Key Auth plugin instance with the given configuration.
pub fn create_key_auth_plugin(cfg: YamlValue) -> Result<Arc<dyn ProxyPlugin>> {
    let config: PluginConfig =
        serde_yaml::from_value(cfg).or_err_with(ReadError, || "Invalid key auth plugin config")?;

    config
        .validate()
        .or_err_with(ReadError, || "Invalid key auth plugin config")?;

    Ok(Arc::new(PluginKeyAuth { config }))
}

/// Configuration for the Key Auth plugin.
#[derive(Default, Debug, Serialize, Deserialize, Validate)]
struct PluginConfig {
    #[serde(default = "PluginConfig::default_header")]
    header: String,
    #[serde(default = "PluginConfig::default_query")]
    query: String,
    #[validate(length(min = 1))]
    key: String,
    #[serde(default = "PluginConfig::default_hide_credentials")]
    hide_credentials: bool,
}

impl PluginConfig {
    fn default_header() -> String {
        "apikey".to_string()
    }

    fn default_query() -> String {
        "apikey".to_string()
    }

    fn default_hide_credentials() -> bool {
        false
    }
}

/// Key Auth plugin implementation.
pub struct PluginKeyAuth {
    config: PluginConfig,
}

#[async_trait]
impl ProxyPlugin for PluginKeyAuth {
    fn name(&self) -> &str {
        PLUGIN_NAME
    }

    fn priority(&self) -> i32 {
        2500
    }

    async fn request_filter(&self, session: &mut Session, _ctx: &mut ProxyContext) -> Result<bool> {
        let mut from_header = true;
        let value = request::get_req_header_value(session.req_header(), &self.config.header)
            .unwrap_or_else(|| {
                from_header = false;
                request::get_query_value(session.req_header(), &self.config.query)
                    .unwrap_or_default()
            });

        // match key
        if value.is_empty() || value != self.config.key {
            let msg = "Invalid user authorization";

            let mut header = ResponseHeader::build(StatusCode::UNAUTHORIZED, None)?;
            header.insert_header(header::CONTENT_LENGTH, msg.len().to_string())?;
            header.insert_header("WWW-Authenticate", "ApiKey error=\"invalid_key\"")?;
            session
                .write_response_header(Box::new(header), false)
                .await?;
            session.write_response_body(Some(msg.into()), true).await?;
            return Ok(true);
        }

        // hide key
        if self.config.hide_credentials {
            if from_header {
                session.req_header_mut().remove_header(&self.config.header);
            } else {
                let _ =
                    request::remove_query_from_header(session.req_header_mut(), &self.config.query);
            }
        }

        Ok(false)
    }
}
