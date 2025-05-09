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

/// Global map to store global rules, initialized lazily.
#[derive(Debug, Clone)]
pub struct MCPRouteMetaInfo {
    /// OpenAPI Operation ID, unique identifier for the route.
    pub operation_id: String,
    /// OpenAPI Path, the path of the route.
    pub path: Uri,
    /// OpenAPI Method, the HTTP method of the route.
    pub method: Method,
    /// Upstream ID, the upstream ID of the route.
    pub upstream_id: Option<String>,
    /// Headers, the additional headers to be added to the request.
    pub headers: Option<HashMap<String, String>>,
}

impl MCPRouteMetaInfo {
    /// Get the headers to be added to the request.
    pub fn get_headers(&self) -> HashMap<String, String> {
        self.headers.clone().unwrap_or_default()
    }
}

/// Fetch the global MCP route meta info by id.
/// ### Arguments
/// * `id` - The id of the route.
/// ### Returns
/// * `Option<Arc<MCPRouteMetaInfo>>` - The global MCP route meta info.
/// ### Errors
/// * `None` - If the route is not found.
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
/// MCP OpenAPI Config
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct MCPOpenAPIConfig {
    /// Upstream ID for the OpenAPI route.
    pub upstream_id: Option<String>, // upstream id
    /// Upstream configuration for the OpenAPI route.
    pub upstream_config: Option<Upstream>,
    /// Path for the OpenAPI route.
    pub path: String,
}
