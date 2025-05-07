use http::StatusCode;
use pingora::{proxy::Session, Result};
use pingora_proxy::ProxyHttp;
use serde_json::Map;

use crate::{
    jsonrpc::{JSONRPCRequest, JSONRPCResponse},
    service::mcp::MCPProxyService,
    sse_event::SseEvent,
    types::{ListPromptsResult, Prompt, PromptArgument, RequestId},
};

pub async fn request_processing(
    _ctx: &mut <MCPProxyService as ProxyHttp>::CTX,
    session_id: &str,
    mcp_proxy: &MCPProxyService,
    session: &mut Session,
    request: &JSONRPCRequest,
    stream: bool,
) -> Result<bool> {
    let request_id = request.id.clone().unwrap_or(RequestId::Integer(0));
    // if request.id.is_some() {
    //     request_id = request.id.unwrap();
    // }
    match request.method.as_str() {
        "prompts/list" => {
            log::info!("prompts/list");

            let result = ListPromptsResult {
                meta: Map::new(),
                next_cursor: None,
                prompts: vec![
                    Prompt {
                        name: "[mock data]current-time".to_string(),
                        description: Some(
                            "[mock data]Display current time in the city".to_string(),
                        ),
                        arguments: vec![PromptArgument {
                            name: "city".to_string(),
                            description: Some("City name".to_string()),
                            required: Some(true),
                        }],
                    },
                    Prompt {
                        name: "[mock data]analyze-code".to_string(),
                        description: Some(
                            "[mock data]Analyze code for potential improvements".to_string(),
                        ),
                        arguments: vec![PromptArgument {
                            name: "language".to_string(),
                            description: Some("Programming language".to_string()),
                            required: Some(true),
                        }],
                    },
                ],
            };

            let res = JSONRPCResponse::new(request_id, serde_json::to_value(result).unwrap());
            if stream {
                let event =
                    SseEvent::new_event(session_id, "message", &serde_json::to_string(&res).unwrap());
                let _ = mcp_proxy.tx.send(event);
                mcp_proxy.response_accepted(session).await?;
            } else {
                mcp_proxy.response(session, StatusCode::OK, serde_json::to_string(&res).unwrap()).await?;
            }
            Ok(true)
        }
        "prompts/get" => {
            // let res = JSONRPCResponse::new(
            //     request_id,
            //     serde_json::to_value(result).unwrap(),
            // );
            mcp_proxy.response_accepted(session).await?;
            Ok(true)
        }
        _ => {
            if stream {
                let _ = mcp_proxy.tx.send(SseEvent::new(session_id, "Accepted"));
                mcp_proxy.response_accepted(session).await?;
            } else {
                mcp_proxy.response(session, StatusCode::OK, serde_json::to_string("{}").unwrap()).await?;
            }
            Ok(true)
        }
    }
    // Ok(false)
}
