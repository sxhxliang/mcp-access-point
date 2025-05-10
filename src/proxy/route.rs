use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use dashmap::DashMap;
use log::debug;
use matchit::{InsertError, Router as MatchRouter};
use once_cell::sync::Lazy;
use pingora_core::upstreams::peer::HttpPeer;
use pingora_error::{Error, Result};
use pingora_proxy::Session;

use crate::{
    config::{self, Identifiable},
    plugin::{build_plugin, ProxyPlugin},
    utils::request::get_request_host,
};

use super::{
    service::service_fetch,
    upstream::{upstream_fetch, ProxyUpstream},
    MapOperations, ProxyPluginExecutor,
};

/// Proxy route.
///
/// Manages routing of requests to appropriate proxy load balancers.
#[derive(Clone)]
pub struct ProxyRoute {
    pub inner: config::Route,
    pub upstream: Option<Arc<ProxyUpstream>>,
    pub plugins: Vec<Arc<dyn ProxyPlugin>>,
}

impl From<config::Route> for ProxyRoute {
    /// Creates a new `ProxyRoute` instance from a `Route` configuration.
    fn from(value: config::Route) -> Self {
        Self {
            inner: value,
            upstream: None,
            plugins: Vec::new(),
        }
    }
}

impl Identifiable for ProxyRoute {
    fn id(&self) -> &str {
        &self.inner.id
    }

    fn set_id(&mut self, id: String) {
        self.inner.id = id;
    }
}

impl ProxyRoute {
    pub fn new_with_upstream_and_plugins(
        route: config::Route,
        work_stealing: bool,
    ) -> Result<Self> {
        let mut proxy_route = ProxyRoute {
            inner: route.clone(),
            upstream: None,
            plugins: Vec::with_capacity(route.plugins.len()),
        };

        // Configure upstream
        if let Some(upstream_config) = route.upstream {
            let proxy_upstream =
                ProxyUpstream::new_with_health_check(upstream_config, work_stealing)?;
            proxy_route.upstream = Some(Arc::new(proxy_upstream));
        }

        // Load plugins
        for (name, value) in route.plugins {
            let plugin = build_plugin(&name, value)?;
            proxy_route.plugins.push(plugin);
        }

        Ok(proxy_route)
    }

    /// Gets the upstream for the route.
    pub fn resolve_upstream(&self) -> Option<Arc<ProxyUpstream>> {
        self.upstream
            .clone()
            .or_else(|| {
                self.inner
                    .upstream_id
                    .as_ref()
                    .and_then(|id| upstream_fetch(id.as_str()))
            })
            .or_else(|| {
                self.inner
                    .service_id
                    .as_ref()
                    .and_then(|id| service_fetch(id).and_then(|s| s.resolve_upstream()))
            })
    }

    /// Gets the list of hosts for the route.
    fn get_hosts(&self) -> Vec<String> {
        let hosts = self.inner.get_hosts();
        if !hosts.is_empty() {
            hosts
        } else if let Some(service) = self
            .inner
            .service_id
            .as_ref()
            .and_then(|id| service_fetch(id.as_str()))
        {
            service.inner.hosts.clone()
        } else {
            vec![]
        }
    }

    pub fn select_http_peer<'a>(&'a self, session: &'a mut Session) -> Result<Box<HttpPeer>> {
        self.resolve_upstream()
            .ok_or_else(|| Error::new_str("Failed to retrieve upstream configuration for route"))
            .and_then(|upstream| {
                upstream
                    .select_backend(session)
                    .ok_or_else(|| Error::new_str("Unable to determine backend for the request"))
            })
            .and_then(|mut backend| {
                backend
                    .ext
                    .get_mut::<HttpPeer>()
                    .map(|peer| {
                        self.set_timeout(peer);
                        Box::new(peer.clone())
                    })
                    .ok_or_else(|| Error::new_str("Missing selected backend metadata for HttpPeer"))
            })
    }

    /// Sets the timeout for an `HttpPeer` based on the route configuration.
    fn set_timeout(&self, p: &mut HttpPeer) {
        if let Some(config::Timeout {
            connect,
            read,
            send,
        }) = self.inner.timeout
        {
            p.options.connection_timeout = Some(Duration::from_secs(connect));
            p.options.read_timeout = Some(Duration::from_secs(read));
            p.options.write_timeout = Some(Duration::from_secs(send));
        }
    }

    /// Builds a `ProxyPluginExecutor` by combining plugins from both a route and its associated service.
    ///
    /// # Arguments
    /// - `self`: A reference-counted pointer to a `ProxyRoute` instance containing route-specific plugins.
    ///
    /// # Returns
    /// - `Arc<ProxyPluginExecutor>`: A reference-counted pointer to a `ProxyPluginExecutor` that manages the merged plugin list.
    ///
    /// # Process
    /// - Retrieves route-specific plugins from the `route`.
    /// - If the route is associated with a service (via `service_id`), retrieves service-specific plugins.
    /// - Combines the route and service plugins, ensuring unique entries by their name.
    /// - Sorts the merged plugin list by priority in descending order.
    /// - Constructs and returns the `ProxyPluginExecutor` instance.
    ///
    /// # Notes
    /// - Plugins from the route take precedence over those from the service in case of naming conflicts.
    ///   If a plugin with the same name exists in both route and service, only the route's plugin is retained.
    pub fn build_plugin_executor(&self) -> Arc<ProxyPluginExecutor> {
        let mut plugin_map: HashMap<String, Arc<dyn ProxyPlugin>> = HashMap::new();

        // Merge route and service plugins
        let service_plugins = self
            .inner
            .service_id
            .as_deref()
            .and_then(service_fetch)
            .map_or_else(Vec::new, |service| service.plugins.clone());

        for plugin in self.plugins.iter().chain(service_plugins.iter()) {
            plugin_map
                .entry(plugin.name().to_string())
                .or_insert_with(|| plugin.clone());
        }

        // Sort by priority in descending order
        let mut merged_plugins: Vec<_> = plugin_map.into_values().collect();
        merged_plugins.sort_by_key(|b| std::cmp::Reverse(b.priority()));

        Arc::new(ProxyPluginExecutor {
            plugins: merged_plugins,
        })
    }
}

#[derive(Default)]
pub struct MatchEntry {
    /// Router for non-host URI matching
    non_host_uri: MatchRouter<Vec<Arc<ProxyRoute>>>,
    /// Router for host URI matching
    host_uris: MatchRouter<MatchRouter<Vec<Arc<ProxyRoute>>>>,
}

impl MatchEntry {
    fn insert_into_router(
        router: &mut MatchRouter<Vec<Arc<ProxyRoute>>>,
        uri: &str,
        proxy_route: Arc<ProxyRoute>,
    ) -> Result<(), InsertError> {
        match router.at_mut(uri) {
            Ok(routes) => {
                routes.value.push(proxy_route);
                // Sort routes by priority (higher priority values take precedence)
                routes
                    .value
                    .sort_by(|a, b| b.inner.priority.cmp(&a.inner.priority));
            }
            Err(_) => {
                router.insert(uri, vec![proxy_route])?;
            }
        }
        Ok(())
    }

    /// Inserts a route into the match entry.
    pub fn insert_route(&mut self, proxy_route: Arc<ProxyRoute>) -> Result<(), InsertError> {
        let hosts = proxy_route.get_hosts();
        let uris = proxy_route.inner.get_uris();

        if hosts.is_empty() {
            // Insert for non-host URIs
            for uri in &uris {
                Self::insert_into_router(&mut self.non_host_uri, uri, proxy_route.clone())?;
            }
        } else {
            // Insert for host URIs
            // Host strings are reversed to enable suffix/wildcard matching with matchit's prefix-based router
            // (e.g., "*.example.com" becomes "moc.elpmaxe.*" for efficient matching)
            for host in hosts.iter() {
                let reversed_host = host.chars().rev().collect::<String>();
                let inner_router = self.host_uris.at_mut(reversed_host.as_str());

                let inner_router = match inner_router {
                    Ok(router) => router.value,
                    Err(_) => {
                        let new_router = MatchRouter::new();
                        self.host_uris.insert(reversed_host.clone(), new_router)?;
                        self.host_uris.at_mut(reversed_host.as_str()).unwrap().value
                    }
                };

                for uri in &uris {
                    Self::insert_into_router(inner_router, uri, proxy_route.clone())?;
                }
            }
        }

        Ok(())
    }

    /// Matches a request to a route.
    pub fn match_request(
        &self,
        session: &mut Session,
    ) -> Option<(BTreeMap<String, String>, Arc<ProxyRoute>)> {
        let host = get_request_host(session.req_header());
        let uri = session.req_header().uri.path();
        let method = session.req_header().method.as_str();

        log::debug!(
            "match request: host={:?}, uri={:?}, method={:?}",
            host,
            uri,
            method
        );

        // Attempt to match using host_uris if a valid host is provided
        // Host is reversed to match the format used during insertion (e.g., "moc.elpmaxe.*")
        if let Some(reversed_host) = host
            .filter(|h| !h.is_empty())
            .map(|h| h.chars().rev().collect::<String>())
        {
            if let Ok(v) = self.host_uris.at(&reversed_host) {
                if let Some(result) = Self::match_uri_method(v.value, uri, method) {
                    return Some(result);
                }
            }
        }

        // Fall back to non-host URI matching
        Self::match_uri_method(&self.non_host_uri, uri, method)
    }

    /// Matches a URI to a route.
    fn match_uri_method(
        match_router: &MatchRouter<Vec<Arc<ProxyRoute>>>,
        uri: &str,
        method: &str,
    ) -> Option<(BTreeMap<String, String>, Arc<ProxyRoute>)> {
        if let Ok(v) = match_router.at(uri) {
            let params: BTreeMap<String, String> = v
                .params
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();

            for route in v.value.iter() {
                if route.inner.methods.is_empty() {
                    return Some((params, route.clone()));
                }

                // Match method
                if route.inner.methods.iter().any(|m| m.to_string() == method) {
                    return Some((params, route.clone()));
                }
            }
        }
        None
    }
}

/// Global map to store global rules, initialized lazily.
pub static ROUTE_MAP: Lazy<DashMap<String, Arc<ProxyRoute>>> = Lazy::new(DashMap::new);
static GLOBAL_ROUTE_MATCH: Lazy<ArcSwap<MatchEntry>> =
    Lazy::new(|| ArcSwap::new(Arc::new(MatchEntry::default())));

pub fn global_route_match_fetch() -> Arc<MatchEntry> {
    GLOBAL_ROUTE_MATCH.load().clone()
}

/// Reloads the global route match by iterating over all routes in the `ROUTE_MAP` and inserting them
/// into a new `MatchEntry` instance. If any route insertion fails, the error is logged, but the
/// process continues to ensure that partial failures do not prevent the remaining routes from being
/// processed. Finally, the newly populated `MatchEntry` is stored in the `GLOBAL_ROUTE_MATCH` as an
/// `Arc` for shared ownership and thread safety.
///
/// This function does not take any parameters and does not return any value.
pub fn reload_global_route_match() {
    // Initialize a new `MatchEntry` instance with default values.
    let mut matcher = MatchEntry::default();

    // Iterate over each route in the `ROUTE_MAP` and attempt to insert it into the `matcher`.
    for route in ROUTE_MAP.iter() {
        debug!("Inserting route: {}", route.inner.id);
        if let Err(e) = matcher.insert_route(route.clone()) {
            // Log an error if the route insertion fails, but continue processing other routes.
            log::error!("Failed to insert route {}: {}", route.inner.id, e);
        }
    }

    // Store the populated `matcher` in the `GLOBAL_ROUTE_MATCH` as an `Arc` for shared access.
    GLOBAL_ROUTE_MATCH.store(Arc::new(matcher));
}

/// Loads routes from the given configuration.
pub fn load_static_routes(config: &config::Config) -> Result<()> {
    let proxy_routes: Vec<Arc<ProxyRoute>> = config
        .routes
        .iter()
        .map(|route| {
            log::info!("Configuring Route: {}", route.id);
            match ProxyRoute::new_with_upstream_and_plugins(
                route.clone(),
                config.pingora.work_stealing,
            ) {
                Ok(proxy_route) => Ok(Arc::new(proxy_route)),
                Err(e) => {
                    log::error!("Failed to configure Route {}: {}", route.id, e);
                    Err(e)
                }
            }
        })
        .collect::<Result<Vec<_>>>()?;

    ROUTE_MAP.reload_resources(proxy_routes);

    reload_global_route_match();

    Ok(())
}
