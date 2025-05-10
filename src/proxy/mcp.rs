use std::sync::{Arc, Mutex};

use dashmap::DashMap;
use once_cell::sync::Lazy;
use pingora_error::Result;
use serde_json::Map;

use crate::{
    config::{self, Identifiable, MCPOpenAPIConfig, MCP_ROUTE_META_INFO_MAP},
    openapi::OpenApiSpec,
    plugin::{build_plugin, ProxyPlugin},
    types::ListToolsResult,
    utils::file::read_from_local_or_remote,
};

use super::{
    upstream::{upstream_fetch, ProxyUpstream},
    MapOperations,
};

/// Global map to store global rules, initialized lazily.
pub static MCP_TOOLS_MAP: Lazy<Arc<Mutex<ListToolsResult>>> = Lazy::new(|| {
    Arc::new(Mutex::new(ListToolsResult {
        meta: Map::new(),
        next_cursor: None,
        tools: vec![],
    }))
});

pub fn global_openapi_tools_fetch() -> Option<ListToolsResult> {
    // Lock the Mutex and clone the inner value to return as Arc
    MCP_TOOLS_MAP.lock().ok().map(|tools| tools.clone())
}

pub fn reload_global_openapi_tools(
    openapi_content: String,
) -> Result<ListToolsResult, Box<dyn std::error::Error>> {
    let spec: OpenApiSpec = OpenApiSpec::new(openapi_content)?;
    let (tools, mcp_route_metas) = spec.load_openapi()?;
    for (key, value) in mcp_route_metas {
        MCP_ROUTE_META_INFO_MAP.insert(key, value);
    }
    // Lock the Mutex and update the global tools map
    let mut map = MCP_TOOLS_MAP.lock().map_err(|e| e.to_string())?;
    *map = ListToolsResult {
        meta: Map::new(),
        next_cursor: None,
        tools: tools.tools.clone(),
    };

    Ok(tools)
}

pub fn reload_global_openapi_tools_from_config(
    mcp_cfgs: Vec<MCPOpenAPIConfig>,
) -> Result<ListToolsResult, Box<dyn std::error::Error>> {
    let mut tools: ListToolsResult = ListToolsResult {
        meta: Map::new(),
        next_cursor: None,
        tools: vec![],
    };
    for mcp_cfg in mcp_cfgs {
        let (_, content) = read_from_local_or_remote(&mcp_cfg.path)?;
        let mut spec: OpenApiSpec = OpenApiSpec::new(content)?;

        if let Some(upstream_id) = mcp_cfg.upstream_id.clone() {
            spec.upstream_id = Some(upstream_id);
        } else {
            log::warn!("No upstream_id found in openapi content");
        }

        spec.mcp_config = Some(mcp_cfg);

        if tools.tools.is_empty() {
            let (new_tools, mcp_route_metas) = spec.load_openapi()?;
            tools = new_tools;
            for (key, value) in mcp_route_metas {
                MCP_ROUTE_META_INFO_MAP.insert(key, value);
            }
        } else {
            let (new_tools, mcp_route_metas) = spec.load_openapi()?;
            tools.tools.extend(new_tools.tools); // Append new tool
            for (key, value) in mcp_route_metas {
                MCP_ROUTE_META_INFO_MAP.insert(key, value);
            }
        }
    }
    // Lock the Mutex and update the global tools map
    let mut map = MCP_TOOLS_MAP.lock().map_err(|e| e.to_string())?;
    *map = ListToolsResult {
        meta: Map::new(),
        next_cursor: None,
        tools: tools.tools.clone(),
    };

    Ok(tools)
}