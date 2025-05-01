use std::{collections::HashMap, sync::Arc};

use arc_swap::ArcSwap;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use pingora_error::Result;

use crate::{
    config::{self, Identifiable},
    plugin::{build_plugin, ProxyPlugin},
};

use super::{MapOperations, ProxyPluginExecutor};

/// Represents a proxy service that manages upstreams.
pub struct ProxyGlobalRule {
    pub inner: config::GlobalRule,
    pub plugins: Vec<Arc<dyn ProxyPlugin>>,
}

impl Identifiable for ProxyGlobalRule {
    fn id(&self) -> &str {
        &self.inner.id
    }

    fn set_id(&mut self, id: String) {
        self.inner.id = id;
    }
}

impl ProxyGlobalRule {
    pub fn new_with_plugins(rule: config::GlobalRule) -> Result<Self> {
        let mut proxy_global_rule = ProxyGlobalRule {
            inner: rule.clone(),
            plugins: Vec::with_capacity(rule.plugins.len()),
        };

        // Load plugins and log each one
        for (name, value) in rule.plugins {
            log::info!("Loading plugin: {}", name);
            let plugin = build_plugin(&name, value)?;
            proxy_global_rule.plugins.push(plugin);
        }

        Ok(proxy_global_rule)
    }
}

/// Global map to store global rules, initialized lazily.
pub static GLOBAL_RULE_MAP: Lazy<DashMap<String, Arc<ProxyGlobalRule>>> = Lazy::new(DashMap::new);
static GLOBAL_PLUGIN: Lazy<ArcSwap<ProxyPluginExecutor>> =
    Lazy::new(|| ArcSwap::new(Arc::new(ProxyPluginExecutor::default())));

pub fn global_plugin_fetch() -> Arc<ProxyPluginExecutor> {
    GLOBAL_PLUGIN.load().clone()
}

/// Reloads ProxyPluginExecutor with deduplicated plugins sorted by priority.
pub fn reload_global_plugin() {
    // Use a HashMap to deduplicate plugins by name
    let mut unique_plugins: HashMap<String, Arc<dyn ProxyPlugin>> = HashMap::new();

    // Iterate through all ProxyGlobalRule in GLOBAL_RULE_MAP
    for rule in GLOBAL_RULE_MAP.iter() {
        for plugin in &rule.plugins {
            // Deduplicate by plugin name; only one instance of each plugin is kept
            // (the last encountered instance from any GlobalRule is retained)
            let plugin_name = plugin.name();
            unique_plugins.insert(plugin_name.to_string(), plugin.clone());
        }
    }

    // Collect deduplicated plugins and sort by priority
    let mut plugins: Vec<_> = unique_plugins.into_values().collect();

    // Sort plugins by priority (higher priority first)
    plugins.sort_by_key(|b| std::cmp::Reverse(b.priority()));

    // Update GLOBAL_PLUGIN with new executor
    GLOBAL_PLUGIN.store(Arc::new(ProxyPluginExecutor { plugins }));
}

/// Loads services from the given configuration.
pub fn load_static_global_rules(config: &config::Config) -> Result<()> {
    let proxy_global_rules: Vec<Arc<ProxyGlobalRule>> = config
        .global_rules
        .iter()
        .map(|rule| {
            log::info!("Configuring GlobalRule: {}", rule.id);

            // Attempt to create a ProxyGlobalRule with plugins
            match ProxyGlobalRule::new_with_plugins(rule.clone()) {
                Ok(proxy_global_rule) => Ok(Arc::new(proxy_global_rule)),
                Err(e) => {
                    log::error!("Failed to configure GlobalRule {}: {}", rule.id, e);
                    Err(e)
                }
            }
        })
        .collect::<Result<Vec<_>>>()?;

    // Reload global rules into the map
    GLOBAL_RULE_MAP.reload_resources(proxy_global_rules);
    reload_global_plugin();

    Ok(())
}