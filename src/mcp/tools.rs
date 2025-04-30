use std::{str::FromStr, sync::Arc};

use http::Uri;
use pingora::{proxy::Session, Result};
use pingora_proxy::ProxyHttp;
use serde_json::Map;

use crate::{
    config::{self, global_mcp_route_meta_info_fetch},
    jsonrpc::{CallToolRequestParam, JSONRPCRequest, JSONRPCResponse},
    openapi::global_openapi_tools_fetch,
    proxy::route,
    service::mcp::MCPProxyService,
    sse_event::SseEvent,
    types::{CallToolResult, CallToolResultContentItem, RequestId, TextContent},
    utils::request::{merge_path_query, replace_dynamic_params},
};

pub async fn request_processing(
    ctx: &mut <MCPProxyService as ProxyHttp>::CTX,
    session_id: &str,
    mcp_proxy: &MCPProxyService,
    session: &mut Session,
    request: &JSONRPCRequest,
) -> Result<bool> {
    let request_id = request.id.clone().unwrap_or(RequestId::Integer(0));
    match request.method.as_str() {
        "tools/list" => {
            let list_tools = global_openapi_tools_fetch();
            match list_tools {
                Some(tools) => {
                    let res =
                        JSONRPCResponse::new(request_id, serde_json::to_value(tools).unwrap());

                    let event = SseEvent::new_event(
                        session_id,
                        "message",
                        &serde_json::to_string(&res).unwrap(),
                    );
                    let _ = mcp_proxy.tx.send(event);
                    mcp_proxy.response_accepted(session).await?;
                    Ok(true)
                }
                None => {
                    log::warn!("not found tool");
                    Ok(false)
                }
            }
        }
        "tools/call" => {
            let _ = session
                .req_header_mut()
                .insert_header("upstream_peer", "127.0.0.1:8090");
            log::debug!("uri {}", session.req_header().uri.path());

            let req_params = request.params.clone().unwrap();
            let params: CallToolRequestParam = serde_json::from_value(req_params).unwrap();
            log::debug!("params {:?}", params);
            // match route_proxy
            let route_meta_info = global_mcp_route_meta_info_fetch(&params.name);

            log::debug!("route_meta_info {:?}", route_meta_info);
            log::debug!("tools/call");
            match route_meta_info {
                Some(route_meta_info) => {
                    let arguments = &params.arguments.unwrap();
                    let new_path = replace_dynamic_params(route_meta_info.path.path(), arguments);
                    log::debug!("new_path {:?}", new_path);
                    // let query_params = json_to_uri_query(arguments);
                    let path_and_query = merge_path_query(&new_path, "");
                    log::debug!("new_path_and_query {:?}", path_and_query);

                    // add headers from upstream config
                    if let Some(upstream_id) = &route_meta_info.upstream_id {
                        log::info!("route_meta_info upstream: {:#?}", route_meta_info);

                        let route_cfg = config::Route {
                            id: String::new(),
                            upstream_id: Some(upstream_id.clone()),
                            uri: Some(route_meta_info.path.path().to_string()),
                            methods: vec![config::HttpMethod::from_http_method(
                                &route_meta_info.method,
                            )
                            .unwrap()],
                            ..Default::default()
                        };

                        log::info!("route upstream route_cfg: {:#?}", route_cfg);
                        ctx.route_mcp = Some(Arc::new(route::ProxyRoute::from(route_cfg)));
                        // add headers from upstream config
                        let _ = session
                            .req_header_mut()
                            .insert_header("upstream_id", upstream_id);
                        for (key, value) in route_meta_info.get_headers() {
                            let _ = session.req_header_mut().insert_header(key, value);
                        }
                    }

                    session
                        .req_header_mut()
                        .set_method(route_meta_info.method.clone());
                    session
                        .req_header_mut()
                        .set_uri(Uri::from_str(&path_and_query).unwrap());
                    // do not remove_header("Content-Length")
                    session.req_header_mut().remove_header("Content-Type");

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
                    let event = SseEvent::new_event(
                        session_id,
                        "message",
                        &serde_json::to_string(&res).unwrap(),
                    );

                    let _ = mcp_proxy.tx.send(event);
                    mcp_proxy.response_accepted(session).await?;
                    Ok(true)
                }
            }
        }
        _ => {
            let _ = mcp_proxy.tx.send(SseEvent::new(session_id, "Accepted"));
            mcp_proxy.response_accepted(session).await?;
            Ok(true)
        }
    }
    // Ok(false)
}
