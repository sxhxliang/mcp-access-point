pub mod discovery;
pub mod event;
pub mod global_rule;
pub mod route;
pub mod service;
pub mod ssl;
pub mod upstream;

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
    time::Instant,
};

use dashmap::DashMap;
use crate::plugin::ProxyPluginExecutor;

use route::ProxyRoute;

/// Proxy context.
///
/// Holds the context for each request.
pub struct ProxyContext {
    pub route: Option<Arc<ProxyRoute>>,
    pub route_params: Option<BTreeMap<String, String>>,
    pub route_mcp: Option<Arc<ProxyRoute>>,
    pub tries: usize,
    pub request_start: Instant,
    pub plugin: Arc<ProxyPluginExecutor>,
    pub global_plugin: Arc<ProxyPluginExecutor>,
    pub vars: HashMap<String, String>,
}

impl Default for ProxyContext {
    fn default() -> Self {
        Self {
            route: None,
            route_params: None,
            route_mcp: None,
            tries: 0,
            request_start: Instant::now(),
            plugin: Arc::new(ProxyPluginExecutor::default()),
            global_plugin: Arc::new(ProxyPluginExecutor::default()),
            vars: HashMap::new(),
        }
    }
}

pub trait Identifiable {
    fn id(&self) -> String;
    fn set_id(&mut self, id: String);
}

pub trait MapOperations<T> {
    fn reload_resource(&self, resources: Vec<Arc<T>>);

    fn insert_resource(&self, resource: Arc<T>);
}

impl<T> MapOperations<T> for DashMap<String, Arc<T>>
where
    T: Identifiable,
{
    // reload_resource：根据新的资源更新 map，删除不在 resources 中的条目
    fn reload_resource(&self, resources: Vec<Arc<T>>) {
        // Log the old and new resources
        for resource in resources.iter() {
            log::info!("Inserting/Updating resource: {}", resource.id());
        }

        let resource_ids: HashSet<String> = resources.iter().map(|r| r.id()).collect();
        self.retain(|key, _| resource_ids.contains(key));

        for resource in resources {
            let key = resource.id();
            log::info!("Inserting resource with id: {}", key);
            self.insert(key, resource);
        }
    }

    // insert_resource：插入新的资源
    fn insert_resource(&self, resource: Arc<T>) {
        self.insert(resource.id(), resource);
    }
}
