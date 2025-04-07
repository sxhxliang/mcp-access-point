mod notifications;
mod prompts;
mod resources;
mod sampling;
mod tools;


use std::collections::HashMap;

use crate::{
    config::{SERVER_NAME, SERVER_VERSION}, openapi::openapi_to_tools, proxy::ModelContextProtocolProxy, sse_event::SseEvent, types::{Implementation, InitializeResult, JSONRPCRequest, JSONRPCResponse, PromptsCapability, ResourcesCapability, ServerCapabilities, ToolsCapability, LATEST_PROTOCOL_VERSION}
};

use pingora::{proxy::Session, Result};

pub async fn request_processing(
    session_id: &str,
    mcp_proxy: &ModelContextProtocolProxy,
    session: &mut Session,
    request: &JSONRPCRequest,
) -> Result<bool> {

    // Match the request method and delegate processing
    match request.method.as_str() {
        "initialize" => {
            log::info!("using request method: {}", request.method);

            let result = InitializeResult {
                protocol_version: LATEST_PROTOCOL_VERSION.to_string(),
                capabilities: ServerCapabilities {
                    experimental: Some(HashMap::new()),
                    logging: None,
                    prompts: Some(PromptsCapability {
                        list_changed: false,
                    }),
                    resources: Some(ResourcesCapability {
                        subscribe: false,
                        list_changed: false,
                    }),
                    tools: Some(ToolsCapability {
                        list_changed: false,
                    }),
                },
                server_info: Implementation {
                    name: SERVER_NAME.to_string(),
                    version: SERVER_VERSION.to_string(),
                },
                instructions: None,
            };

            let res = JSONRPCResponse::new(request.id.unwrap(), serde_json::to_value(result).unwrap());
            let event =
                SseEvent::new_event(session_id, "message", &serde_json::to_string(&res).unwrap());

            let _ = mcp_proxy.tx.send(event);
            mcp_proxy.response_accepted(session).await?;
            Ok(true)
        }

        "ping" | "notifications/initialized" 
        | "notifications/cancelled" | "notifications/roots/list_changed" 
        | "completion/complete" => 
            return notifications::request_processing(session_id, mcp_proxy, session, request).await,

        "tools/list" => {
            let data = openapi_to_tools().await.unwrap();
            let res = JSONRPCResponse::new(request.id.unwrap(), serde_json::to_value(data).unwrap());

            let event =
                SseEvent::new_event(session_id, "message", &serde_json::to_string(&res).unwrap());
            let _ = mcp_proxy.tx.send(event);
            mcp_proxy.response_accepted(session).await?;
            Ok(true)
        }
        "tools/call" => {
            return tools::request_processing(session_id, mcp_proxy, session, request).await;
        }

        "resources/list" | "resources/read" | "resources/templates/list" => 
            return resources::request_processing(session_id, mcp_proxy, session, request).await,

        "prompts/list" | "prompts/get" => 
            return prompts::request_processing(session_id, mcp_proxy, session, request).await,

        _ => {
            log::info!("Unknown method called: {}", request.method);
            Ok(false) // Gracefully handle unknown methods
        }
    }
}
