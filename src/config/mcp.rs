use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashMap;
use once_cell::sync::Lazy;

use http::{Method, Uri};
use serde::{Deserialize, Serialize};
use serde_yaml::Value as YamlValue;

use crate::types::{Prompt, Resource, Tool, ToolInputSchema};

use super::Upstream;

/// Global map to store global rules, initialized lazily.
pub static MCP_ROUTE_META_INFO_MAP: Lazy<DashMap<String, Arc<MCPRouteMetaInfo>>> =
    Lazy::new(DashMap::new);

/// Global map to store global rules, initialized lazily.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MCPRouteMetaInfo {
    /// OpenAPI Operation ID, unique identifier for the route.
    pub operation_id: String,
    /// OpenAPI Path, the path of the route.
    #[serde(default)]
    pub meta: MCPMetaInfo,
    pub uri: String,
    /// OpenAPI Method, the HTTP method of the route.
    pub method: String,
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

    pub fn uri(&self) -> Uri {
        self.uri.parse::<Uri>().unwrap()
    }
    pub fn method(&self) -> Method {
        Method::from_bytes(self.method.as_bytes()).unwrap()
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
                log::debug!("mcp route with id '{id}' found");
                Some(route.value().clone())
            } else {
                log::warn!("mcp route with id '{id}' not found");
                None
            }
        }
        None => {
            log::warn!("mcp route with id '{id}' not found");
            None
        }
    }
}
/// MCP OpenAPI Config
#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MCPService {
    #[serde(default)]
    pub id: String,
    /// Upstream ID for the OpenAPI route.
    pub upstream_id: Option<String>, // upstream id
    /// Upstream configuration for the OpenAPI route.
    pub upstream: Option<Upstream>,
    /// Path for the OpenAPI route.
    /// if the path is set, the service will be enabled.
    pub path: Option<String>,
    /// routes for the mcp server.
    /// if the routes/route_ids/path arw not set, the service will be disabled.
    pub routes: Option<Vec<MCPRouteMetaInfo>>,
    /// routes id for the mcp server.
    pub route_ids: Option<Vec<String>>,
    /// service plugins for the mcp server.
    #[serde(default)]
    pub plugins: HashMap<String, YamlValue>,
}

/// MCP Meta Info for MCP.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged, rename_all = "lowercase")]
pub enum MCPMetaInfo {
    ToolInfo(Tool),
    PromptInfo(Prompt),
    ResourceInfo(Resource),
}
impl Default for MCPMetaInfo {
    fn default() -> Self {
        Self::ToolInfo(Tool {
            name: "".to_string(),
            annotations: None,
            description: None,
            input_schema: ToolInputSchema {
                properties: HashMap::new(),
                required: Vec::new(),
                type_: "object".to_string(),
            },
        })
    }
}

/// implement PartialEq for MCPMetaInfo
/// alaways return true, because we don't need to compare the meta info.
impl PartialEq for MCPMetaInfo {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}
impl Eq for MCPMetaInfo {}
