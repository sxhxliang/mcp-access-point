use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use http::{header, StatusCode};
use pingora_error::{ErrorType::ReadError, OrErr, Result};
use pingora_http::ResponseHeader;
use pingora_proxy::Session;
use serde::{Deserialize, Serialize};
use serde_yaml::Value as YamlValue;

use crate::proxy::ProxyContext;

use super::ProxyPlugin;

pub const PLUGIN_NAME: &str = "echo";

/// Creates an Echo plugin instance with the given configuration.
pub fn create_echo_plugin(cfg: YamlValue) -> Result<Arc<dyn ProxyPlugin>> {
    let config: PluginConfig =
        serde_yaml::from_value(cfg).or_err_with(ReadError, || "Invalid echo plugin config")?;

    Ok(Arc::new(PluginEcho { config }))
}

/// Configuration for the Echo plugin.
/// Specifies the response body and optional headers.
#[derive(Default, Debug, Serialize, Deserialize)]
struct PluginConfig {
    /// The response body that will be echoed.
    body: String,

    /// Additional response headers to include in the echoed response.
    #[serde(default)]
    headers: HashMap<String, String>,
}

/// Echo plugin implementation.
pub struct PluginEcho {
    config: PluginConfig,
}

#[async_trait]
impl ProxyPlugin for PluginEcho {
    fn name(&self) -> &str {
        PLUGIN_NAME
    }

    fn priority(&self) -> i32 {
        412
    }

    async fn request_filter(&self, session: &mut Session, _ctx: &mut ProxyContext) -> Result<bool> {
        let mut resp = ResponseHeader::build(StatusCode::OK, None)?;

        // Insert headers from the config
        for (k, v) in &self.config.headers {
            resp.insert_header(k.clone(), v.clone())?;
        }

        // Insert Content-Length header
        resp.insert_header(header::CONTENT_LENGTH, self.config.body.len().to_string())?;

        // Write response header to the session
        session.write_response_header(Box::new(resp), false).await?;

        // Write response body to the session
        session
            .write_response_body(Some(self.config.body.clone().into()), true)
            .await?;

        Ok(true)
    }
}
