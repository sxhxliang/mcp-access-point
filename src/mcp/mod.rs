mod notifications;
mod prompts;
mod resources;
mod sampling;
mod tools;

use std::collections::HashMap;

use crate::{
    config::{SERVER_NAME, SERVER_VERSION},
    service::mcp::MCPProxyService,
    sse_event::SseEvent,
    types::{
        Implementation, InitializeResult, ServerCapabilities, ServerCapabilitiesPrompts, ServerCapabilitiesResources, ServerCapabilitiesTools, 
    },
    jsonrpc::{JSONRPCRequest, JSONRPCResponse, LATEST_PROTOCOL_VERSION}
};

use http::StatusCode;
use pingora::{proxy::Session, Result};
use pingora_proxy::ProxyHttp;
use serde_json::Map;

// 2024-11-05 specification protocol;
pub async fn request_processing(
    ctx: &mut <MCPProxyService as ProxyHttp>::CTX,
    session_id: &str,
    mcp_proxy: &MCPProxyService,
    session: &mut Session,
    request: &JSONRPCRequest,
) -> Result<bool> {
    // Match the request method and delegate processing
    match request.method.as_str() {
        "initialize" => {
            log::info!("using request method: {}", request.method);

            let result = InitializeResult {
                meta: Map::new(),
                protocol_version: LATEST_PROTOCOL_VERSION.to_string(),
                capabilities: ServerCapabilities {
                    completions: Map::new(),
                    experimental: HashMap::new(),
                    logging: Map::new(),
                    prompts: Some(ServerCapabilitiesPrompts {
                        list_changed: None,
                    }),
                    resources: Some(ServerCapabilitiesResources {
                        subscribe: None,
                        list_changed: None,
                    }),
                    tools: Some(ServerCapabilitiesTools {
                        list_changed: None,
                    }),
                },
                server_info: Implementation {
                    name: SERVER_NAME.to_string(),
                    version: SERVER_VERSION.to_string(),
                },
                instructions: None,
            };

            let res =
                JSONRPCResponse::new(request.id.clone().unwrap(), serde_json::to_value(result).unwrap());
            let event =
                SseEvent::new_event(session_id, "message", &serde_json::to_string(&res).unwrap());

            let _ = mcp_proxy.tx.send(event);
            mcp_proxy.response_accepted(session).await?;
            Ok(true)
        }

        "ping"
        | "notifications/initialized"
        | "notifications/cancelled"
        | "notifications/roots/list_changed"
        | "completion/complete" => {
            return notifications::request_processing(ctx, session_id, mcp_proxy, session, request, true).await
        }

        "tools/list" | "tools/call" => {
            return tools::request_processing(ctx, session_id, mcp_proxy, session, request, true).await
        }

        "resources/list" | "resources/read" | "resources/templates/list" => {
            return resources::request_processing(ctx, session_id, mcp_proxy, session, request, true).await
        }

        "prompts/list" | "prompts/get" => {
            return prompts::request_processing(ctx, session_id, mcp_proxy, session, request, true).await
        }

        _ => {
            log::info!("Unknown method called: {}", request.method);
            Ok(false) // Gracefully handle unknown methods
        }
    }
}

pub async fn request_processing_streamable_http(
    ctx: &mut <MCPProxyService as ProxyHttp>::CTX,
    session_id: &str,
    mcp_proxy: &MCPProxyService,
    session: &mut Session,
    request: &JSONRPCRequest,
) -> Result<bool> {
    // Match the request method and delegate processing
    match request.method.as_str() {
        "initialize" => {
            log::info!("using request method: {}", request.method);

            let result = InitializeResult {
                meta: Map::new(),
                protocol_version: LATEST_PROTOCOL_VERSION.to_string(),
                capabilities: ServerCapabilities {
                    completions: Map::new(),
                    experimental: HashMap::new(),
                    logging: Map::new(),
                    prompts: Some(ServerCapabilitiesPrompts {
                        list_changed: None,
                    }),
                    resources: Some(ServerCapabilitiesResources {
                        subscribe: None,
                        list_changed: None,
                    }),
                    tools: Some(ServerCapabilitiesTools {
                        list_changed: None,
                    }),
                },
                server_info: Implementation {
                    name: SERVER_NAME.to_string(),
                    version: SERVER_VERSION.to_string(),
                },
                instructions: None,
            };

            let res =
                JSONRPCResponse::new(request.id.clone().unwrap(), serde_json::to_value(result).unwrap());
            // let event =
                // SseEvent::new_event(session_id, "message", &serde_json::to_string(&res).unwrap());
            mcp_proxy.response(session, StatusCode::OK, serde_json::to_string(&res).unwrap()).await?;
            // let _ = mcp_proxy.tx.send(event);
            // mcp_proxy.response_accepted(session).await?;
            Ok(true)
        }

        "ping"
        | "notifications/initialized"
        | "notifications/cancelled"
        | "notifications/roots/list_changed"
        | "completion/complete" => {
            return notifications::request_processing(ctx, session_id, mcp_proxy, session, request, false).await
        }

        "tools/list" | "tools/call" => {
            return tools::request_processing(ctx, session_id, mcp_proxy, session, request, false).await
        }

        "resources/list" | "resources/read" | "resources/templates/list" => {
            return resources::request_processing(ctx, session_id, mcp_proxy, session, request, false).await
        }

        "prompts/list" | "prompts/get" => {
            return prompts::request_processing(ctx, session_id, mcp_proxy, session, request, false).await
        }

        _ => {
            log::info!("Unknown method called: {}", request.method);
            Ok(false) // Gracefully handle unknown methods
        }
    }
}
