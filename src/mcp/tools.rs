use std::str::FromStr;

use http::Uri;
use pingora::{proxy::Session, Result};

use crate::{
    openapi::global_openapi_tools_fetch,
    proxy::{route::global_openapi_route_fetch, ModelContextProtocolProxy},
    sse_event::SseEvent,
    types::{
        CallToolRequestParam, CallToolResult, Content, JSONRPCRequest, JSONRPCResponse, TextContent,
    },
    utils::{merge_path_query, replace_dynamic_params},
};

pub async fn request_processing(
    session_id: &str,
    mcp_proxy: &ModelContextProtocolProxy,
    session: &mut Session,
    request: &JSONRPCRequest,
) -> Result<bool> {
    let mut request_id = 0;
    if request.id.is_some() {
        request_id = request.id.unwrap();
    }
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
                    return Ok(true);
                }
                None => {
                    log::warn!("not found tool");
                    return Ok(false);
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
            let route_proxy = global_openapi_route_fetch(&params.name);
            log::debug!("route_proxy {:?}", route_proxy);
            log::debug!("tools/call");
            match route_proxy {
                Some(route) => {
                    let arguments = &params.arguments.unwrap();
                    let new_path = replace_dynamic_params(route.path.path(), arguments);
                    log::debug!("new_path {:?}", new_path);
                    // let query_params = json_to_uri_query(arguments);
                    let path_and_query = merge_path_query(&new_path, "");
                    log::debug!("new_path_and_query {:?}", path_and_query);

                    session.req_header_mut().set_method(route.method.clone());
                    session
                        .req_header_mut()
                        .set_uri(Uri::from_str(&path_and_query).unwrap());
                    // do not remove_header("Content-Length")
                    session.req_header_mut().remove_header("Content-Type");

                    return Ok(false);
                }
                None => {
                    log::warn!("not found tool {}", params.name);
                    let result = CallToolResult {
                        content: vec![Content::Text(TextContent {
                            text: "not found tool".to_string(),
                            annotations: None,
                        })],
                        is_error: Some(false),
                    };
                    let res =
                        JSONRPCResponse::new(request_id, serde_json::to_value(result).unwrap());
                    let event = SseEvent::new_event(
                        session_id,
                        "message",
                        &serde_json::to_string(&res).unwrap(),
                    );

                    let _ = mcp_proxy.tx.send(event);
                    mcp_proxy.response_accepted(session).await?;
                    return Ok(true);
                }
            }
        }
        _ => {
            let _ = mcp_proxy.tx.send(SseEvent::new(session_id, "Accepted"));
            mcp_proxy.response_accepted(session).await?;
            return Ok(true);
        }
    }
    Ok(false)
}
