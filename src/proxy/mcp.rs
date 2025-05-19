use std::sync::Arc;

use dashmap::DashMap;
use once_cell::sync::Lazy;
use pingora_error::Result;
use serde_json::Map;

use crate::{
    config::{self, Identifiable, MCP_ROUTE_META_INFO_MAP}, openapi::OpenApiSpec, plugin::ProxyPlugin, proxy::upstream::upstream_fetch, types::{ListToolsResult, Tool}, utils::file::read_from_local_or_remote
};

use super::{route::ProxyRoute, upstream::ProxyUpstream, MapOperations};

pub struct MCPToolsList {
    name: String,
    tools_list: ListToolsResult,
}

impl Identifiable for MCPToolsList {
    fn id(&self) -> &str {
        &self.name
    }

    fn set_id(&mut self, id: String) {
        self.name = id;
    }
}
/// Global map to store services, initialized lazily.
pub static MCP_SERVICE_TOOLS_MAP: Lazy<DashMap<String, Arc<MCPToolsList>>> =
    Lazy::new(DashMap::new);

pub fn global_openapi_tools_fetch_by_id(id: &str) -> Option<ListToolsResult> {
    MCP_SERVICE_TOOLS_MAP
        .get(id)
        .map(|service| service.value().tools_list.clone())
        .or_else(|| {
            log::warn!("Service with id '{}' not found", id);
            Some(ListToolsResult::default())
        })
}

pub fn global_openapi_tools_fetch() -> Option<ListToolsResult> {
    let mut tools = ListToolsResult::default();
    MCP_SERVICE_TOOLS_MAP
        .iter()
        .for_each(|service| tools.tools.extend(service.value().tools_list.tools.clone()));
    Some(tools)
}

/// Global map to store mcp services
/// key: service id, value: MCPToolsList
pub fn reload_global_openapi_tools_from_service_config(
    service: &config::MCPService,
) -> Result<
    (
        ListToolsResult,
        DashMap<String, Arc<config::MCPRouteMetaInfo>>,
    ),
    Box<dyn std::error::Error>,
> {
    let mut tools: ListToolsResult = ListToolsResult {
        meta: Map::new(),
        next_cursor: None,
        tools: vec![],
    };
    let meta_info: DashMap<String, Arc<config::MCPRouteMetaInfo>> = DashMap::new();

    if let Some(path) = &service.path {
        let (_, content) = read_from_local_or_remote(path)?;
        let mut spec: OpenApiSpec = OpenApiSpec::new(content)?;
        spec.set_mcp_config(service.clone());
        if tools.tools.is_empty() {
            let (new_tools, mcp_route_metas) = spec.load_openapi()?;
            tools = new_tools;
            for (key, value) in mcp_route_metas {
                meta_info.insert(key, value);
            }
        } else {
            let (new_tools, mcp_route_metas) = spec.load_openapi()?;
            tools.tools.extend(new_tools.tools); // Append new tool
            for (key, value) in mcp_route_metas {
                meta_info.insert(key, value);
            }
        }
    }

    Ok((tools, meta_info))
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
        let mut tools_meta_info: DashMap<String, Arc<config::MCPRouteMetaInfo>> = DashMap::new();
        // Configure upstream
        if let Some(upstream_config) = &service.upstream.clone() {
            let mut upstream_config = upstream_config.clone();
            if let Some(upstream_id) = &service.upstream_id {
                // need initialize upstream config from upstream config with upstream id
                // if upstream config has upstream id, the upstream config will be merged with upstream config with upstream id
                let upstream = upstream_fetch(upstream_id);
                if upstream.is_none() {
                    log::warn!("upstream with id '{}' not found", upstream_id);
                    // panic!("upstream with id '{}' not found", upstream_id);
                }
                upstream_config.merge(upstream.unwrap().inner.clone());
            }

            let proxy_upstream =
                ProxyUpstream::new_with_health_check(upstream_config, work_stealing)?;
            proxy_mcp_service.upstream = Some(Arc::new(proxy_upstream));
        }
        // Load plugins
        // for (name, value) in &service.plugins {
        //     let plugin = build_plugin(name, value.clone())?;
        //     proxy_mcp_service.plugins.push(plugin);
        // }
        //  Configure routes
        if let Some(routes_config) = &service.routes {
            for cfg in routes_config {
                let mut cfg = cfg.clone();
                if cfg.upstream_id.is_none() {
                    cfg.upstream_id = service.upstream_id.clone();
                    log::info!("route:\n {:#?}", cfg);
                };

                match &cfg.meta {
                    config::MCPMetaInfo::ToolInfo(tool) => tools.push(tool.clone()),
                    config::MCPMetaInfo::PromptInfo(prompt) => todo!(),
                    config::MCPMetaInfo::ResourceInfo(resource) => todo!(),
                };
                tools_meta_info.insert(cfg.operation_id.clone(), Arc::new(cfg.clone()));
            }
        }
        // configure openapi tools
        if service.path.is_some() {
            if let Ok((other_tools, other_meta_info)) =
                reload_global_openapi_tools_from_service_config(&service)
            {
                tools.extend(other_tools.tools); // Append new tool
                tools_meta_info.extend(other_meta_info);
            }
        }

        let list_tools = ListToolsResult {
            meta: Map::new(),
            next_cursor: None,
            tools: tools.clone(),
        };

        //  insert meta info to global map
        //  tool call from meta info by operation_id
        for pair in tools_meta_info.into_iter() {
            MCP_ROUTE_META_INFO_MAP.insert(pair.0, pair.1);
        }
        // log::info!("tools_meta_info: {:#?}", MCP_ROUTE_META_INFO_MAP);
        //  insert tools to global map
        // list tools from global map by service id
        MCP_SERVICE_TOOLS_MAP.insert(
            service.id.clone(),
            MCPToolsList {
                tools_list: list_tools,
                name: service.id.clone(),
            }
            .into(),
        );

        // 配置 upstream
        if let Some(ref upstream_config) = service.upstream {
            let proxy_upstream =
                ProxyUpstream::new_with_health_check(upstream_config.clone(), work_stealing)?;
            proxy_mcp_service.upstream = Some(Arc::new(proxy_upstream));
        }

        Ok(proxy_mcp_service)
    }
    pub fn get_tools(&self) -> Option<ListToolsResult> {
        global_openapi_tools_fetch_by_id(self.id())
    }
    pub fn get_meta_info(&self, operation_id: &str) -> Option<Arc<config::MCPRouteMetaInfo>> {
        MCP_ROUTE_META_INFO_MAP
            .get(operation_id)
            .map(|route| route.value().clone())
    }
    // pub fn resolve_route(&self, route_id: &str) -> Option<Route> {
    //     self.routes.get(route_id).clone()
    // }
    /// Gets the Route for the service.
    pub fn resolve_upstream(&self) -> Option<Arc<ProxyUpstream>> {
        self.upstream
            .clone()
            .or_else(|| self.inner.upstream_id.as_deref().and_then(upstream_fetch))
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
    log::info!("Loaded {} MCP Service(s)", proxy_mcp_services.len());
    MCP_SERVICE_MAP.reload_resources(proxy_mcp_services);

    Ok(())
}
