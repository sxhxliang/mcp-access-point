use std::{collections::HashMap, sync::Arc};

use arc_swap::ArcSwap;
use dashmap::DashMap;
use once_cell::sync::Lazy;
use pingora_error::Result;

use crate::{
    config,
    plugin::{build_plugin, ProxyPlugin, ProxyPluginExecutor},
};

use super::{Identifiable, MapOperations};

/// Represents a proxy service that manages upstreams.
pub struct ProxyGlobalRule {
    pub inner: config::GlobalRule,
    pub plugins: Vec<Arc<dyn ProxyPlugin>>,
}

impl Identifiable for ProxyGlobalRule {
    fn id(&self) -> String {
        self.inner.id.clone()
    }

    fn set_id(&mut self, id: String) {
        self.inner.id = id;
    }
}

impl From<config::GlobalRule> for ProxyGlobalRule {
    fn from(value: config::GlobalRule) -> Self {
        Self {
            inner: value,
            plugins: Vec::new(),
        }
    }
}

impl ProxyGlobalRule {
    pub fn new_with_plugins(rule: config::GlobalRule) -> Result<Self> {
        let mut proxy_global_rule = Self::from(rule.clone());

        // Load plugins and log each one
        for (name, value) in rule.plugins {
            log::info!("Loading plugin: {}", name); // Add logging for each plugin
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

/// reload ProxyPluginExecutor
pub fn reload_global_plugin() {
    // 创建一个 HashMap 用来去重插件
    let mut unique_plugins: HashMap<String, Arc<dyn ProxyPlugin>> = HashMap::new();

    // 遍历 GLOBAL_RULE_MAP 中的所有 ProxyGlobalRule
    for rule in GLOBAL_RULE_MAP.iter() {
        for plugin in &rule.plugins {
            // 使用 plugin.name() 作为唯一标识符，去重
            let plugin_name = plugin.name();
            unique_plugins.insert(plugin_name.to_string(), plugin.clone());
        }
    }

    // 从 HashMap 获取去重后的插件并根据 priority 排序
    let mut plugins: Vec<_> = unique_plugins.into_values().collect();

    // 按照 ProxyPlugin.priority() 排序，优先级大的排前面
    plugins.sort_by_key(|b| std::cmp::Reverse(b.priority()));

    // 创建并返回 ProxyPluginExecutor
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
    GLOBAL_RULE_MAP.reload_resource(proxy_global_rules);
    reload_global_plugin();

    Ok(())
}

pub fn global_rule_fetch(id: &str) -> Option<Arc<ProxyGlobalRule>> {
    match GLOBAL_RULE_MAP.get(id) {
        Some(rule) => Some(rule.value().clone()),
        None => {
            log::warn!("Global rule with id '{}' not found", id);
            None
        }
    }
}
