use std::sync::Arc;
use crate::config::{Config, Identifiable, MCPService, Route, Service as ConfigService, Upstream, GlobalRule, SSL};
use crate::proxy::{
    global_rule::{ProxyGlobalRule, GLOBAL_RULE_MAP, reload_global_plugin},
    mcp::{ProxyMCPService, MCP_SERVICE_MAP},
    route::{ProxyRoute, ROUTE_MAP, reload_global_route_match},
    service::{ProxyService, SERVICE_MAP},
    ssl::{ProxySSL, SSL_MAP, reload_global_ssl_match},
    upstream::{ProxyUpstream, UPSTREAM_MAP},
    MapOperations,
};
use crate::proxy::event::InnerComparable; // Reuse this trait

// Helper function to create proxy resources if they are new or changed.
// T: Config type (e.g., Route, Upstream)
// P: Proxy type (e.g., ProxyRoute, ProxyUpstream)
// F: Factory function to create P from T
fn generate_proxy_resources<T, P, F>(
    new_items: &[T],
    old_items_map: &std::collections::HashMap<String, T>, // For quick lookup of old items
    proxy_map: &impl MapOperations<P>, // To get existing Arc<P> for comparison
    create_proxy_fn: F,
    work_stealing: bool,
) -> Vec<Arc<P>>
where
    T: Identifiable + Clone + PartialEq, // PartialEq for easy comparison
    P: Identifiable + InnerComparable<T>,
    F: Fn(T, bool) -> pingora_error::Result<P>,
{
    new_items
        .iter()
        .filter_map(|new_item_cfg| {
            let id = new_item_cfg.id().to_string();
            match old_items_map.get(&id) {
                Some(old_item_cfg) if old_item_cfg == new_item_cfg => {
                    // Item is unchanged, try to reuse existing proxy object from the map
                    proxy_map.get(&id)
                }
                _ => { // Item is new or changed
                    log::info!("Configuring (new/changed) resource ID: {}", id);
                    match create_proxy_fn(new_item_cfg.clone(), work_stealing) {
                        Ok(proxy_obj) => Some(Arc::new(proxy_obj)),
                        Err(e) => {
                            log::error!("Failed to create proxy for resource ID {}: {}", id, e);
                            None
                        }
                    }
                }
            }
        })
        .collect()
}


// Helper to build a HashMap from a slice of Identifiable items for quick lookup.
fn to_hashmap<T: Identifiable + Clone>(items: &[T]) -> std::collections::HashMap<String, T> {
    items.iter().map(|item| (item.id().to_string(), item.clone())).collect()
}

pub(super) fn apply_config_changes(
    new_config: &Config,
    old_config: &Config, // The previously active configuration
    work_stealing: bool,
) {
    log::info!("Applying configuration changes...");

    // MCP Services
    let old_mcps_map = to_hashmap(&old_config.mcps);
    let new_proxy_mcps = generate_proxy_resources(
        &new_config.mcps,
        &old_mcps_map,
        &*MCP_SERVICE_MAP,
        ProxyMCPService::new_with_routes_upstream_and_plugins,
        work_stealing,
    );
    MCP_SERVICE_MAP.reload_resources(new_proxy_mcps);
    log::info!("MCP services updated.");

    // SSLs
    let old_ssls_map = to_hashmap(&old_config.ssls);
    let new_proxy_ssls = generate_proxy_resources(
        &new_config.ssls,
        &old_ssls_map,
        &*SSL_MAP,
        |ssl, _| Ok(ProxySSL::from(ssl.clone())), // Pass SSL by value
        work_stealing, // work_stealing might not be used by all factories
    );
    SSL_MAP.reload_resources(new_proxy_ssls);
    reload_global_ssl_match();
    log::info!("SSL configurations updated.");

    // Upstreams
    let old_upstreams_map = to_hashmap(&old_config.upstreams);
    let new_proxy_upstreams = generate_proxy_resources(
        &new_config.upstreams,
        &old_upstreams_map,
        &*UPSTREAM_MAP,
        ProxyUpstream::new_with_health_check,
        work_stealing,
    );
    UPSTREAM_MAP.reload_resources(new_proxy_upstreams);
    log::info!("Upstreams updated.");

    // Services (ConfigService to avoid conflict with pingora::services::Service)
    let old_services_map = to_hashmap(&old_config.services);
    let new_proxy_services = generate_proxy_resources(
        &new_config.services,
        &old_services_map,
        &*SERVICE_MAP,
        ProxyService::new_with_upstream_and_plugins,
        work_stealing,
    );
    SERVICE_MAP.reload_resources(new_proxy_services);
    log::info!("Services updated.");

    // GlobalRules
    let old_global_rules_map = to_hashmap(&old_config.global_rules);
    let new_proxy_global_rules = generate_proxy_resources(
        &new_config.global_rules,
        &old_global_rules_map,
        &*GLOBAL_RULE_MAP,
        |gr, _| ProxyGlobalRule::new_with_plugins(gr),
        work_stealing,
    );
    GLOBAL_RULE_MAP.reload_resources(new_proxy_global_rules);
    reload_global_plugin();
    log::info!("Global rules updated.");

    // Routes
    // Routes depend on upstreams and services being configured, so they often come last or after dependencies.
    let old_routes_map = to_hashmap(&old_config.routes);
    let new_proxy_routes = generate_proxy_resources(
        &new_config.routes,
        &old_routes_map,
        &*ROUTE_MAP,
        ProxyRoute::new_with_upstream_and_plugins,
        work_stealing,
    );
    ROUTE_MAP.reload_resources(new_proxy_routes);
    reload_global_route_match();
    log::info!("Routes updated.");

    log::info!("Configuration changes applied successfully.");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{self, Config, Route, Upstream, Service as ConfigService, GlobalRule, SSL, HttpMethod, MCPService, mcp::MCPServiceRoute, Listener, AccessPointConfig};
    use crate::proxy::{ROUTE_MAP, UPSTREAM_MAP, SERVICE_MAP, GLOBAL_RULE_MAP, SSL_MAP, MCP_SERVICE_MAP};
    use std::collections::HashMap;
    use pingora_core::server::configuration::ServerConf; // For Config.pingora

    // Helper to create a basic default config
    fn basic_config() -> Config {
        Config {
            pingora: ServerConf::default(),
            access_point: AccessPointConfig {
                listeners: vec![Listener {
                    address: "0.0.0.0:8080".parse().unwrap(),
                    tls: None,
                    offer_h2: false,
                    offer_h2c: false,
                }],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    // Helper to create a simple route
    fn create_route(id: &str, path: &str) -> Route {
        Route {
            id: id.to_string(),
            uri: Some(path.to_string()),
            upstream_id: Some("test_upstream".to_string()), // Assume a default upstream for simplicity
            ..Default::default()
        }
    }

    // Helper to create a simple upstream
    fn create_upstream(id: &str, node_addr: &str) -> Upstream {
        let mut nodes = HashMap::new();
        nodes.insert(node_addr.to_string(), 1);
        Upstream {
            id: id.to_string(),
            nodes,
            ..Default::default()
        }
    }


    #[test]
    fn test_apply_no_changes() {
        let old_conf = basic_config();
        let new_conf = old_conf.clone();

        // Pre-populate maps based on old_conf to simulate initial load
        // For simplicity, let's assume maps are empty and apply_config_changes populates them.
        // Or, we can manually populate them here if generate_proxy_resources relies on existing Arcs.
        // For this test, starting with empty maps is fine as no items should change.
        UPSTREAM_MAP.reload_resources(vec![]); // Clear map
        ROUTE_MAP.reload_resources(vec![]);    // Clear map

        apply_config_changes(&new_conf, &old_conf, false);

        // Assert that maps are still empty or contain only what was in basic_config (nothing in this case)
        assert_eq!(UPSTREAM_MAP.iter().count(), 0);
        assert_eq!(ROUTE_MAP.iter().count(), 0);
        // Add asserts for other maps (SERVICE_MAP, GLOBAL_RULE_MAP, SSL_MAP, MCP_SERVICE_MAP)
    }

    #[test]
    fn test_apply_add_route_and_upstream() {
        let mut old_conf = basic_config();
        let mut new_conf = old_conf.clone();

        let upstream1 = create_upstream("test_upstream", "127.0.0.1:8000");
        let route1 = create_route("route1", "/app1");

        new_conf.upstreams.push(upstream1.clone());
        new_conf.routes.push(route1.clone());

        // Clear maps before test
        UPSTREAM_MAP.reload_resources(vec![]);
        ROUTE_MAP.reload_resources(vec![]);

        apply_config_changes(&new_conf, &old_conf, false);

        assert_eq!(UPSTREAM_MAP.iter().count(), 1);
        assert!(UPSTREAM_MAP.get("test_upstream").is_some());
        assert_eq!(UPSTREAM_MAP.get("test_upstream").unwrap().inner.nodes, upstream1.nodes);

        assert_eq!(ROUTE_MAP.iter().count(), 1);
        assert!(ROUTE_MAP.get("route1").is_some());
        assert_eq!(ROUTE_MAP.get("route1").unwrap().inner.uri, route1.uri);

        // Now, let old_conf be new_conf and add another route to new_conf
        old_conf = new_conf.clone();
        let route2 = create_route("route2", "/app2");
        let mut newer_conf = new_conf.clone(); // Use new_conf as base for newer_conf
        newer_conf.routes.push(route2.clone());

        apply_config_changes(&newer_conf, &old_conf, false);

        assert_eq!(UPSTREAM_MAP.iter().count(), 1); // Upstream should remain
        assert_eq!(ROUTE_MAP.iter().count(), 2);
        assert!(ROUTE_MAP.get("route1").is_some());
        assert!(ROUTE_MAP.get("route2").is_some());
        assert_eq!(ROUTE_MAP.get("route2").unwrap().inner.uri, route2.uri);
    }

    #[test]
    fn test_apply_modify_route() {
        let mut old_conf = basic_config();
        let upstream1 = create_upstream("test_upstream", "127.0.0.1:8000");
        let route1_v1 = create_route("route1", "/app1_v1");

        old_conf.upstreams.push(upstream1.clone());
        old_conf.routes.push(route1_v1.clone());

        // Initial load into maps
        apply_config_changes(&old_conf, &basic_config(), false); // Compare with empty to load
        assert_eq!(ROUTE_MAP.get("route1").unwrap().inner.uri, route1_v1.uri);

        let mut new_conf = old_conf.clone();
        let route1_v2 = Route { uri: Some("/app1_v2".to_string()), ..route1_v1.clone() };
        new_conf.routes[0] = route1_v2.clone(); // Modify the first route

        apply_config_changes(&new_conf, &old_conf, false);

        assert_eq!(ROUTE_MAP.iter().count(), 1);
        assert!(ROUTE_MAP.get("route1").is_some());
        assert_eq!(ROUTE_MAP.get("route1").unwrap().inner.uri, route1_v2.uri);
        // Check that the Arc itself might have changed (or not, depending on how ProxyRoute handles updates)
        // This is harder to check without storing pointers. The key is that the *inner* data is updated.
    }

    #[test]
    fn test_apply_delete_route() {
        let mut old_conf = basic_config();
        let upstream1 = create_upstream("test_upstream", "127.0.0.1:8000");
        let route1 = create_route("route1", "/app1");
        let route2 = create_route("route2", "/app2");

        old_conf.upstreams.push(upstream1.clone());
        old_conf.routes.push(route1.clone());
        old_conf.routes.push(route2.clone());

        apply_config_changes(&old_conf, &basic_config(), false); // Initial load
        assert_eq!(ROUTE_MAP.iter().count(), 2);

        let mut new_conf = old_conf.clone();
        new_conf.routes.remove(0); // Remove route1

        apply_config_changes(&new_conf, &old_conf, false);

        assert_eq!(ROUTE_MAP.iter().count(), 1);
        assert!(ROUTE_MAP.get("route1").is_none());
        assert!(ROUTE_MAP.get("route2").is_some());
    }

    // TODO: Add tests for other resource types (Services, GlobalRules, SSLs, MCPs)
    // TODO: Add tests for scenarios where a resource is unchanged (Arc reuse)
    // For Arc reuse, we would need to:
    // 1. Load initial config.
    // 2. Get an Arc<ProxyRoute> from ROUTE_MAP.
    // 3. Create a new config that is identical for that route.
    // 4. Call apply_config_changes.
    // 5. Get the Arc<ProxyRoute> again and assert that Arc::ptr_eq is true.
}
