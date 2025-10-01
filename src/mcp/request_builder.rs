use std::{collections::HashMap, str::FromStr, sync::Arc};

use http::Uri;
use pingora_proxy::Session;

use crate::{
    config::{self, MCPRouteMetaInfo},
    proxy::{route, ProxyContext},
    utils::request::{build_uri_with_path_and_query, flatten_json},
};

/// Builder for constructing upstream requests from tool call parameters
pub struct RequestBuilder;

impl RequestBuilder {
    /// Builds and applies upstream request configuration
    ///
    /// # Arguments
    /// * `ctx` - Proxy context to store route configuration
    /// * `session` - HTTP session to modify
    /// * `route_meta_info` - Route metadata from tool definition
    /// * `arguments` - Tool call arguments
    pub fn build_request(
        ctx: &mut ProxyContext,
        session: &mut Session,
        route_meta_info: Arc<MCPRouteMetaInfo>,
        arguments: &serde_json::Value,
    ) {
        // Build path and query string
        let path_and_query = Self::build_path_and_query(&route_meta_info, arguments);

        // Configure route
        Self::configure_route(ctx, session, &route_meta_info);

        // Set HTTP method and URI
        session
            .req_header_mut()
            .set_method(route_meta_info.method().clone());
        session
            .req_header_mut()
            .set_uri(Uri::from_str(&path_and_query).unwrap());

        // Extract and store request body
        Self::extract_request_body(ctx, &route_meta_info, arguments);
    }

    /// Builds path and query string from route metadata and arguments
    fn build_path_and_query(
        route_meta_info: &MCPRouteMetaInfo,
        arguments: &serde_json::Value,
    ) -> String {
        let mut flattened_params = HashMap::new();
        flatten_json("", arguments, &mut flattened_params);

        log::debug!(
            "Building URL with path: {} and params: {:?}",
            route_meta_info.uri().path(),
            flattened_params
        );

        build_uri_with_path_and_query(route_meta_info.uri().path(), &flattened_params)
    }

    /// Configures route and upstream settings
    fn configure_route(
        ctx: &mut ProxyContext,
        session: &mut Session,
        route_meta_info: &MCPRouteMetaInfo,
    ) {
        if let Some(upstream_id) = &route_meta_info.upstream_id {
            log::info!("Configuring route with upstream: {upstream_id}");

            // Build route configuration
            let route_cfg = config::Route {
                id: String::new(),
                upstream_id: Some(upstream_id.clone()),
                uri: Some(route_meta_info.uri().path().to_string()),
                methods: vec![config::HttpMethod::from_http_method(&route_meta_info.method())
                    .unwrap()],
                headers: route_meta_info.headers.clone(),
                ..Default::default()
            };

            ctx.route = Some(Arc::new(route::ProxyRoute::from(route_cfg)));
            ctx.vars
                .insert("upstream_id".to_string(), upstream_id.to_string());

            // Apply headers from route metadata
            for (key, value) in route_meta_info.get_headers() {
                let _ = session.req_header_mut().insert_header(key, value);
            }
        }
    }

    /// Extracts and stores request body based on HTTP method
    fn extract_request_body(
        ctx: &mut ProxyContext,
        route_meta_info: &MCPRouteMetaInfo,
        arguments: &serde_json::Value,
    ) {
        let method = route_meta_info.method();
        let method_str = method.as_str();
        let methods_with_body = ["POST", "PUT", "PATCH", "DELETE", "OPTIONS"];

        if methods_with_body.contains(&method_str) {
            log::info!("Method {method_str} supports body - extracting from arguments");

            // Extract body from arguments
            if let Some(body_value) = arguments.get("body").cloned().or_else(|| Some(arguments.clone())) {
                if let Ok(new_body_bytes) = serde_json::to_vec(&body_value) {
                    log::info!(
                        "Extracted body for upstream: {}",
                        String::from_utf8_lossy(&new_body_bytes)
                    );

                    // Store in context for request_body_filter
                    ctx.vars.insert(
                        "new_body".to_string(),
                        String::from_utf8_lossy(&new_body_bytes).to_string(),
                    );
                    ctx.vars
                        .insert("new_body_len".to_string(), new_body_bytes.len().to_string());

                    log::info!("Stored extracted body in context for method {method_str}");
                }
            }
        } else {
            // For methods without body (GET, HEAD), ensure no body is sent
            log::info!("Method {method_str} does not support body - ensuring no body is sent");
            ctx.vars.insert("new_body".to_string(), String::new());
            ctx.vars.insert("new_body_len".to_string(), "0".to_string());
        }
    }
}