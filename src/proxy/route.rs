
use std::sync::Arc;

use dashmap::DashMap;
use http::{Uri, Method};
use once_cell::sync::Lazy;

use crate::config::UpstreamConfig;

#[derive(Debug, Clone)]
pub struct ProxyRoute {
    pub operation_id: String,
    pub path: Uri,
    pub method: Method,
    pub upstream: Option<UpstreamConfig>,
}
/// Global map to store global rules, initialized lazily.
pub static MCP_ROUTE_MAP: Lazy<DashMap<String, Arc<ProxyRoute>>> = Lazy::new(DashMap::new);

pub fn global_openapi_route_fetch(id: &str) -> Option<Arc<ProxyRoute>> {
    match MCP_ROUTE_MAP.get(id) {
        Some(route) => {
            if id == route.value().operation_id {
                log::debug!("mcp route with id '{}' found", id);
                Some(route.value().clone())
            } else {
                log::warn!("mcp route with id '{}' not found", id);
                None
            }
        },
        None => {
            log::warn!("mcp route with id '{}' not found", id);
            None
        }
    }
}
