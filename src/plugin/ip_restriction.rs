use std::{net::IpAddr, sync::Arc};

use async_trait::async_trait;
use http::{header, StatusCode};
use ipnetwork::IpNetwork;
use pingora_error::{ErrorType::ReadError, OrErr, Result};
use pingora_http::ResponseHeader;
use pingora_proxy::Session;
use serde::{Deserialize, Serialize};
use serde_yaml::Value as YamlValue;

use crate::proxy::ProxyContext;
use crate::utils::request::get_client_ip;

use super::ProxyPlugin;

pub const PLUGIN_NAME: &str = "ip-restriction";

/// Creates an IP Restriction plugin instance.
pub fn create_ip_restriction_plugin(cfg: YamlValue) -> Result<Arc<dyn ProxyPlugin>> {
    let config: PluginConfig = serde_yaml::from_value(cfg)
        .or_err_with(ReadError, || "Invalid IP restriction plugin config")?;
    Ok(Arc::new(PluginIPRestriction { config }))
}

/// Configuration for the IP Restriction plugin.
#[derive(Default, Debug, Serialize, Deserialize)]
struct PluginConfig {
    /// List of allowed IP networks (whitelist).
    #[serde(default)]
    whitelist: Vec<IpNetwork>,

    /// List of denied IP networks (blacklist).
    #[serde(default)]
    blacklist: Vec<IpNetwork>,

    /// Custom rejection message.
    message: Option<String>,
}

/// IP Restriction Plugin implementation.
pub struct PluginIPRestriction {
    config: PluginConfig,
}

#[async_trait]
impl ProxyPlugin for PluginIPRestriction {
    fn name(&self) -> &str {
        PLUGIN_NAME
    }

    fn priority(&self) -> i32 {
        3000
    }

    async fn request_filter(&self, session: &mut Session, _ctx: &mut ProxyContext) -> Result<bool> {
        let client_ip = get_client_ip(session)
            .parse::<IpAddr>()
            .or_err_with(ReadError, || "Failed to parse client IP")?;

        // Check whitelist
        if !self.config.whitelist.is_empty()
            && !self
                .config
                .whitelist
                .iter()
                .any(|network| network.contains(client_ip))
        {
            return self.reject_request(session).await;
        }

        // Check blacklist
        if self
            .config
            .blacklist
            .iter()
            .any(|network| network.contains(client_ip))
        {
            return self.reject_request(session).await;
        }

        Ok(false)
    }
}

impl PluginIPRestriction {
    /// Rejects the request with a `403 Forbidden` response.
    async fn reject_request(&self, session: &mut Session) -> Result<bool> {
        let mut header = ResponseHeader::build(StatusCode::FORBIDDEN, None)?;

        if let Some(ref msg) = self.config.message {
            header.insert_header(header::CONTENT_LENGTH, msg.len().to_string())?;
            session
                .write_response_header(Box::new(header), false)
                .await?;
            session
                .write_response_body(Some(msg.clone().into()), true)
                .await?;
        } else {
            session
                .write_response_header(Box::new(header), true)
                .await?;
        }

        Ok(true)
    }
}
