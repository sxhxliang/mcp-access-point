//! Module for proxy context and resource management.
//!
//! This module defines the `ProxyContext` used per request and the generic
//! `MapOperations` trait for managing resources in a thread-safe map.

pub mod discovery;
pub mod event;
pub mod global_rule;
pub mod mcp;
pub mod route;
pub mod service;
pub mod ssl;
pub mod upstream;

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
    time::Instant,
};

use async_trait::async_trait;
use bytes::Bytes;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use pingora_error::{Error, Result};
use pingora_http::{RequestHeader, ResponseHeader};
use pingora_proxy::Session;

use crate::{config::Identifiable, plugin::ProxyPlugin};

use route::ProxyRoute;

/// Default empty plugin executor for new ProxyContext.
static DEFAULT_PLUGIN_EXECUTOR: Lazy<Arc<ProxyPluginExecutor>> =
    Lazy::new(|| Arc::new(ProxyPluginExecutor::default()));

/// Holds the context for each proxy request.
pub struct ProxyContext {
    /// The matched proxy route, if any.
    pub route: Option<Arc<ProxyRoute>>,
    /// Parameters extracted from the route pattern.
    pub route_params: Option<BTreeMap<String, String>>,
    pub route_mcp: Option<Arc<ProxyRoute>>,
    pub tries: usize,
    /// Timestamp when the request started.
    pub request_start: Instant,
    /// Executor for route-specific plugins.
    pub plugin: Arc<ProxyPluginExecutor>,
    /// Executor for global plugins.
    pub global_plugin: Arc<ProxyPluginExecutor>,
    /// Custom variables available to plugins.
    pub vars: HashMap<String, String>,
    /// Buffer for the response body from the upstream.
    pub body_buffer: Vec<Bytes>,
}

impl Default for ProxyContext {
    fn default() -> Self {
        Self {
            route: None,
            route_params: None,
            route_mcp: None,
            tries: 0,
            request_start: Instant::now(),
            plugin: DEFAULT_PLUGIN_EXECUTOR.clone(),
            global_plugin: DEFAULT_PLUGIN_EXECUTOR.clone(),
            vars: HashMap::new(),
            body_buffer: Vec::new(),
        }
    }
}

/// Trait for performing common operations on a map of resources.
///
/// Provides methods to fetch, bulk reload, and insert individual resources.
pub trait MapOperations<T> {
    /// Get a resource by its identifier.
    ///
    /// Returns `Some(Arc<T>)` if found, otherwise logs a warning and returns `None`.
    fn get(&self, id: &str) -> Option<Arc<T>>;

    /// Reload the entire set of resources.
    ///
    /// Removes entries not present in `resources`, and inserts or updates all given resources.
    fn reload_resources(&self, resources: Vec<Arc<T>>);

    /// Insert or update a single resource.
    fn insert_resource(&self, resource: Arc<T>);
}

impl<T> MapOperations<T> for DashMap<String, Arc<T>>
where
    T: Identifiable,
{
    fn get(&self, id: &str) -> Option<Arc<T>> {
        if let Some(entry) = self.get(id) {
            Some(entry.clone())
        } else {
            log::warn!("Resource with id '{}' not found", id);
            None
        }
    }

    fn reload_resources(&self, resources: Vec<Arc<T>>) {
        // Log incoming resources
        for resource in &resources {
            log::info!("Resource: {}", resource.id());
        }

        // Build a set of IDs to keep
        let valid_ids: HashSet<String> = resources.iter().map(|r| r.id().to_string()).collect();
        log::info!("Valid IDs: {:?}", valid_ids);
        // Remove entries not in the new set
        self.retain(|key, _| valid_ids.contains(key));

        // Insert or update all resources
        for resource in resources {
            let key = resource.id().to_string();
            log::info!("Inserting or updating resource '{}'", key);
            self.insert(key, resource);
        }
    }

    fn insert_resource(&self, resource: Arc<T>) {
        let key = resource.id();
        log::info!("Inserting resource '{}'", key);
        self.insert(key.to_string(), resource);
    }
}

/// A struct that manages the execution of proxy plugins.
///
/// # Fields
/// - `plugins`: A vector of reference-counted pointers to `ProxyPlugin` instances.
///   These plugins are executed in the order of their priorities, typically determined
///   during the construction of the `ProxyPluginExecutor`.
///
/// # Purpose
/// - This struct is responsible for holding and managing a collection of proxy plugins.
/// - It is typically used to facilitate the execution of plugins in a proxy routing context,
///   where plugins can perform various tasks such as authentication, logging, traffic shaping, etc.
///
/// # Usage
/// - The plugins are expected to be sorted by their priority (in descending order) during
///   the initialization of the `ProxyPluginExecutor`.
#[derive(Default)]
pub struct ProxyPluginExecutor {
    pub plugins: Vec<Arc<dyn ProxyPlugin>>,
}

#[async_trait]
impl ProxyPlugin for ProxyPluginExecutor {
    fn name(&self) -> &str {
        "plugin-executor"
    }

    fn priority(&self) -> i32 {
        0
    }

    async fn request_filter(&self, session: &mut Session, ctx: &mut ProxyContext) -> Result<bool> {
        for plugin in self.plugins.iter() {
            if plugin.request_filter(session, ctx).await? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn early_request_filter(
        &self,
        session: &mut Session,
        ctx: &mut ProxyContext,
    ) -> Result<()> {
        for plugin in self.plugins.iter() {
            plugin.early_request_filter(session, ctx).await?;
        }
        Ok(())
    }

    async fn upstream_request_filter(
        &self,
        session: &mut Session,
        upstream_request: &mut RequestHeader,
        ctx: &mut ProxyContext,
    ) -> Result<()> {
        for plugin in self.plugins.iter() {
            plugin
                .upstream_request_filter(session, upstream_request, ctx)
                .await?;
        }
        Ok(())
    }

    async fn response_filter(
        &self,
        session: &mut Session,
        upstream_response: &mut ResponseHeader,
        ctx: &mut ProxyContext,
    ) -> Result<()> {
        for plugin in self.plugins.iter() {
            plugin
                .response_filter(session, upstream_response, ctx)
                .await?;
        }
        Ok(())
    }

    fn response_body_filter(
        &self,
        session: &mut Session,
        body: &mut Option<Bytes>,
        end_of_stream: bool,
        ctx: &mut ProxyContext,
    ) -> Result<()> {
        for plugin in self.plugins.iter() {
            plugin.response_body_filter(session, body, end_of_stream, ctx)?;
        }
        Ok(())
    }

    async fn logging(&self, session: &mut Session, e: Option<&Error>, ctx: &mut ProxyContext) {
        for plugin in self.plugins.iter() {
            plugin.logging(session, e, ctx).await;
        }
    }
}
