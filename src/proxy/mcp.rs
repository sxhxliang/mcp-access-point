use std::sync::{Arc, Mutex};

use dashmap::DashMap;
use once_cell::sync::Lazy;
use pingora_error::Result;
use serde_json::Map;

use crate::{
    config::{self, Identifiable, MCPService, MCP_ROUTE_META_INFO_MAP},
    openapi::OpenApiSpec,
    plugin::ProxyPlugin,
    types::{ListToolsResult, Tool},
    utils::file::read_from_local_or_remote,
};

use super::{route::ProxyRoute, upstream::ProxyUpstream, MapOperations};

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
    mcp_cfgs: Vec<MCPService>,
) -> Result<ListToolsResult, Box<dyn std::error::Error>> {
    let mut tools: ListToolsResult = ListToolsResult {
        meta: Map::new(),
        next_cursor: None,
        tools: vec![],
    };
    for mcp_cfg in mcp_cfgs {
        if mcp_cfg.path.is_none() {
            log::warn!("No path found in openapi config");
            continue; // Skip if path is not give
        }
        let (_, content) = read_from_local_or_remote(&mcp_cfg.path.clone().unwrap())?;
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

pub fn reload_global_openapi_tools_from_service_config(
    service: &config::MCPService,
) -> Result<ListToolsResult, Box<dyn std::error::Error>> {
    let mut tools: ListToolsResult = ListToolsResult {
        meta: Map::new(),
        next_cursor: None,
        tools: vec![],
    };

    if let Some(path) = &service.path {
        let (_, content) = read_from_local_or_remote(path)?;
        let mut spec: OpenApiSpec = OpenApiSpec::new(content)?;
        spec.mcp_config = Some(service.clone());
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
    let mut map = MCP_TOOLS_MAP.lock().map_err(|e| e.to_string())?;
    *map = ListToolsResult {
        meta: Map::new(),
        next_cursor: None,
        tools: tools.tools.clone(),
    };
    Ok(tools)
}
/// Fetches a mcp service by its ID.
pub fn mcp_service_fetch(id: &str) -> Option<Arc<ProxyMCPService>> {
    match MCP_SERVICE_MAP.get(id) {
        Some(service) => Some(service.value().clone()),
        None => {
            log::warn!("Service with id '{}' not found", id);
            None
        }
    }
}

/// Represents a mcp proxy service that manages upstreams.
pub struct ProxyMCPService {
    /// the service config
    pub inner: config::MCPService,
    /// one mcp service may have many routes, but one route only has one upstream
    pub routes: Option<Vec<Arc<ProxyRoute>>>,
    /// one mcp service has one global upstream,
    /// when the service has many routes, the upstream will be shared by all routes.
    pub upstream: Option<Arc<ProxyUpstream>>,
    pub plugins: Vec<Arc<dyn ProxyPlugin>>,
}

impl Identifiable for ProxyMCPService {
    fn id(&self) -> &str {
        &self.inner.id
    }

    fn set_id(&mut self, id: String) {
        self.inner.id = id;
    }
}

impl ProxyMCPService {
    pub fn new_with_routes_upstream_and_plugins(
        service: config::MCPService,
        work_stealing: bool,
    ) -> Result<Self> {
        let mut proxy_mcp_service = ProxyMCPService {
            inner: service.clone(),
            routes: None,
            upstream: None,
            plugins: Vec::with_capacity(service.plugins.len()),
        };
        log::info!("mcp service:\n {:#?}", service);
        // 配置 routes
        let mut tools: Vec<Tool> = Vec::new();
        if let Some(ref routes_config) = service.routes {
            for cfg in routes_config {
                MCP_ROUTE_META_INFO_MAP.insert(cfg.operation_id.clone(), Arc::new(cfg.clone()));
                match &cfg.meta {
                    config::MCPMetaInfo::ToolInfo(tool) => tools.push(tool.clone()),
                    config::MCPMetaInfo::PromptInfo(prompt) => todo!(),
                    config::MCPMetaInfo::ResourceInfo(resource) => todo!(),
                };
            }
        }

        if service.path.is_some() {
            if let Ok(other_tools) = reload_global_openapi_tools_from_service_config(&service) {
                tools.extend(other_tools.tools); // Append new tool
            }
        }
        let mut map = MCP_TOOLS_MAP.lock().map_err(|e| e.to_string()).unwrap();
        *map = ListToolsResult {
            meta: Map::new(),
            next_cursor: None,
            tools: tools.clone(),
        };

        // 配置 upstream
        if let Some(ref upstream_config) = service.upstream {
            let proxy_upstream =
                ProxyUpstream::new_with_health_check(upstream_config.clone(), work_stealing)?;
            proxy_mcp_service.upstream = Some(Arc::new(proxy_upstream));
        }

        Ok(proxy_mcp_service)
    }

    // pub fn resolve_route(&self, route_id: &str) -> Option<Route> {
    //     self.routes.get(route_id).clone()
    // }
    /// Gets the Route for the service.
    pub fn resolve_upstream(&self) -> Option<Arc<ProxyUpstream>> {
        self.routes.as_ref().and_then(|routes| {
            if routes.is_empty() {
                return None; // No routes, return None
            }

            let first_route = &routes[0]; // Get the first route
            first_route.upstream.clone() // Return the upstream from the first route
        })
    }
}

/// Global map to store services, initialized lazily.
pub static MCP_SERVICE_MAP: Lazy<DashMap<String, Arc<ProxyMCPService>>> = Lazy::new(DashMap::new);

/// Loads services from the given configuration.
pub fn load_static_mcp_services(config: &config::Config) -> Result<()> {
    let proxy_mcp_services: Vec<Arc<ProxyMCPService>> = config
        .mcps
        .iter()
        .map(|service| {
            log::info!("Configuring MCP Service: {}", service.id);
            match ProxyMCPService::new_with_routes_upstream_and_plugins(
                service.clone(),
                config.pingora.work_stealing,
            ) {
                Ok(proxy_mcp_service) => Ok(Arc::new(proxy_mcp_service)),
                Err(e) => {
                    log::error!("Failed to configure Service {}: {}", service.id, e);
                    Err(e)
                }
            }
        })
        .collect::<Result<Vec<_>>>()?;

    MCP_SERVICE_MAP.reload_resources(proxy_mcp_services);

    Ok(())
}
