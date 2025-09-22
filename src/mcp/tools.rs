use std::{str::FromStr, sync::Arc};

use http::Uri;
use pingora::Result;
use pingora_proxy::{ProxyHttp, Session};
use serde_json::Map;

use crate::{
    config::{self, global_mcp_route_meta_info_fetch},
    jsonrpc::{CallToolRequestParam, JSONRPCRequest, JSONRPCResponse},
    mcp::send_json_response,
    proxy::{
        mcp::{global_openapi_tools_fetch, mcp_service_fetch},
        route,
    },
    service::{mcp::MCPProxyService, constants::{MCP_TENANT_ID, NEW_BODY, NEW_BODY_LEN}},
    types::{CallToolResult, CallToolResultContentItem, ListToolsResult, RequestId, TextContent},
    utils::request::build_uri_with_path_and_query,
};

pub async fn request_processing(
    ctx: &mut <MCPProxyService as ProxyHttp>::CTX,
    session_id: &str,
    mcp_proxy: &MCPProxyService,
    session: &mut Session,
    request: &JSONRPCRequest,
    stream: bool, // TODO: Implement stream handling if needed, currently unused in this cod
) -> Result<bool> {
    let request_id = request.id.clone().unwrap_or(RequestId::Integer(0));
    match request.method.as_str() {
        "tools/list" => {
            let list_tools = match ctx.vars.get("MCP_TENANT_ID") {
                Some(tenant_id) => {
                    log::debug!("tools/list--tenant_id {tenant_id:?}");
                    match mcp_service_fetch(tenant_id) {
                        Some(mcp_service) => mcp_service.get_tools(),
                        None => Some(ListToolsResult::default()),
                    }
                }
                None => {
                    log::debug!("tenant_id not found");
                    global_openapi_tools_fetch()
                }
            };
            match list_tools {
                Some(tools) => {
                    let res =
                        JSONRPCResponse::new(request_id, serde_json::to_value(tools).unwrap());
                    send_json_response(mcp_proxy, session, &res, stream, session_id).await?;
                    Ok(true)
                }
                None => {
                    log::warn!("not found tool");
                    Ok(false)
                }
            }
        }
        "tools/call" => {
            log::debug!("uri {}", session.req_header().uri.path());

            let req_params = match request.params.clone() {
                Some(p) => p,
                None => {
                    log::error!("Missing params in tools/call request");
                    let result = CallToolResult {
                        meta: Map::new(),
                        content: vec![CallToolResultContentItem::TextContent(TextContent {
                            type_: "text".to_string(),
                            text: "Missing request parameters".to_string(),
                            annotations: None,
                        })],
                        is_error: Some(true),
                    };
                    let res = JSONRPCResponse::new(
                        request_id.clone(),
                        serde_json::to_value(result).unwrap(),
                    );
                    send_json_response(mcp_proxy, session, &res, stream, session_id).await?;
                    return Ok(true);
                }
            };
            let params: CallToolRequestParam = match serde_json::from_value(req_params) {
                Ok(p) => p,
                Err(e) => {
                    log::error!("Failed to deserialize CallToolRequestParam: {e}");
                    let result = CallToolResult {
                        meta: Map::new(),
                        content: vec![CallToolResultContentItem::TextContent(TextContent {
                            type_: "text".to_string(),
                            text: "Invalid request parameters".to_string(),
                            annotations: None,
                        })],
                        is_error: Some(true),
                    };
                    let res = JSONRPCResponse::new(
                        request_id.clone(),
                        serde_json::to_value(result).unwrap(),
                    );
                    send_json_response(mcp_proxy, session, &res, stream, session_id).await?;
                    return Ok(true);
                }
            };
            log::debug!("params {params:?}");
            // match route_proxy
            // let route_meta_info = global_mcp_route_meta_info_fetch(&params.name);
            let route_meta_info = match ctx.vars.get(MCP_TENANT_ID) {
                Some(tenant_id) => {
                    log::debug!("tools/call--tenant_id {tenant_id:?}");
                    mcp_service_fetch(tenant_id)
                        .unwrap()
                        .get_meta_info(&params.name)
                }
                None => {
                    log::debug!("tenant_id not found");
                    global_mcp_route_meta_info_fetch(&params.name)
                }
            };
            log::debug!("route_meta_info {route_meta_info:#?}");
            log::debug!("tools/call");
            match route_meta_info {
                Some(route_meta_info) => {
                    let arguments = &params.arguments.unwrap();
                    // let new_path = replace_dynamic_params(route_meta_info.uri().path(), arguments);
                    // log::debug!("new_path {new_path:?}");
                    // let query_params = json_to_uri_query(arguments);
                    // let path_and_query = merge_path_query(&new_path, "");
                    let path_and_query = build_uri_with_path_and_query(route_meta_info.uri().path(), &arguments.as_object().unwrap().iter().map(|(k,v)| (k.clone(), v.as_str().unwrap_or("").to_string())).collect());
                    log::debug!("new_path_and_query {path_and_query:?}");

                    // add headers from upstream config
                    if let Some(upstream_id) = &route_meta_info.upstream_id {
                        log::info!("route_meta_info upstream: {route_meta_info:#?}");

                        let route_cfg = config::Route {
                            id: String::new(),
                            upstream_id: Some(upstream_id.clone()),
                            uri: Some(route_meta_info.uri().path().to_string()),
                            methods: vec![config::HttpMethod::from_http_method(
                                &route_meta_info.method(),
                            )
                            .unwrap()],
                            headers: route_meta_info.headers.clone(),
                            ..Default::default()
                        };

                        // log::info!("route upstream route_cfg: {:#?}", route_cfg);
                        ctx.route = Some(Arc::new(route::ProxyRoute::from(route_cfg)));

                        ctx.vars
                            .insert("upstream_id".to_string(), upstream_id.to_string());
                        for (key, value) in route_meta_info.get_headers() {
                            let _ = session.req_header_mut().insert_header(key, value);
                        }
                    }

                    session
                        .req_header_mut()
                        .set_method(route_meta_info.method().clone());
                    session
                        .req_header_mut()
                        .set_uri(Uri::from_str(&path_and_query).unwrap());
                    
                    extract_and_store_request_body(ctx, route_meta_info, arguments);

                    Ok(false)
                }
                None => {
                    log::warn!("not found tool {}", params.name);
                    let result = CallToolResult {
                        meta: Map::new(),
                        content: vec![CallToolResultContentItem::TextContent(TextContent {
                            type_: "text".to_string(),
                            text: "not found tool".to_string(),
                            annotations: None,
                        })],
                        is_error: Some(false),
                    };
                    let res = JSONRPCResponse::new(
                        RequestId::from(0),
                        serde_json::to_value(result).unwrap(),
                    );
                    send_json_response(mcp_proxy, session, &res, stream, session_id).await?;
                    Ok(true)
                }
            }
        }
        _ => {
            let res = JSONRPCResponse::new(request_id, serde_json::to_value("{}").unwrap());
            send_json_response(mcp_proxy, session, &res, stream, session_id).await?;
            Ok(true)
        }
    }
    // Ok(false)
}

fn extract_and_store_request_body(ctx: &mut crate::proxy::ProxyContext, route_meta_info: Arc<config::MCPRouteMetaInfo>, arguments: &serde_json::Value) {
    // Extract and store the proper body for methods that support it
    let method = route_meta_info.method().clone();
    let method_str = method.as_str();
    let methods_with_body = ["POST", "PUT", "PATCH", "DELETE", "OPTIONS"];
                    
    if methods_with_body.contains(&method_str) {
        log::info!("Method {method_str} supports body - extracting from JSON-RPC arguments");
    
        // We already have the arguments from the tool call
        if let Some(body_value) = arguments.get("body").cloned().or_else(|| Some(arguments.clone())) {
            if let Ok(new_body_bytes) = serde_json::to_vec(&body_value) {
                log::info!("Extracted body for upstream: {}", String::from_utf8_lossy(&new_body_bytes));
            
                // Store the extracted body in context for later use in request_body_filter
                ctx.vars.insert(NEW_BODY.to_string(), String::from_utf8_lossy(&new_body_bytes).to_string());
                ctx.vars.insert(NEW_BODY_LEN.to_string(), new_body_bytes.len().to_string());
            
                log::info!("Stored extracted body in context for method {method_str}");
            }
        }
    } else {
        // For methods without body (GET, HEAD), ensure no body is sent
        log::info!("Method {method_str} does not support body - ensuring no body is sent");
        ctx.vars.insert(NEW_BODY.to_string(), String::new());
        ctx.vars.insert(NEW_BODY_LEN.to_string(), "0".to_string());
}
}
