use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashMap;
use once_cell::sync::Lazy;

use http::{Method, Uri};
use serde::{Deserialize, Serialize};

use super::Upstream;

/// Global map to store global rules, initialized lazily.
pub static MCP_ROUTE_META_INFO_MAP: Lazy<DashMap<String, Arc<MCPRouteMetaInfo>>> =
    Lazy::new(DashMap::new);

#[derive(Debug, Clone)]
pub struct MCPRouteMetaInfo {
    pub operation_id: String,
    pub path: Uri,
    pub method: Method,
    pub upstream_id: Option<String>,
    pub headers: Option<HashMap<String, String>>,
}

impl MCPRouteMetaInfo {
    pub fn get_headers(&self) -> HashMap<String, String> {
        self.headers.clone().unwrap_or_default()
    }
}

pub fn global_mcp_route_meta_info_fetch(id: &str) -> Option<Arc<MCPRouteMetaInfo>> {
    match MCP_ROUTE_META_INFO_MAP.get(id) {
        Some(route) => {
            if id == route.value().operation_id {
                log::debug!("mcp route with id '{}' found", id);
                Some(route.value().clone())
            } else {
                log::warn!("mcp route with id '{}' not found", id);
                None
            }
        }
        None => {
            log::warn!("mcp route with id '{}' not found", id);
            None
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct MCPOpenAPIConfig {
    pub upstream_id: Option<String>, // upstream id
    pub upstream_config: Option<Upstream>,
    pub path: String,
}
