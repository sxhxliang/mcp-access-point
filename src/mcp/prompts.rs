use pingora::{proxy::Session, Result};

use crate::{
    service::ModelContextProtocolProxy,
    sse_event::SseEvent,
    types::{JSONRPCRequest, JSONRPCResponse, ListPromptsResult, Prompt, PromptArgument},
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
        "prompts/list" => {
            log::info!("prompts/list");

            let result = ListPromptsResult {
                prompts: vec![
                    Prompt {
                        name: "[mock data]current-time".to_string(),
                        description: Some(
                            "[mock data]Display current time in the city".to_string(),
                        ),
                        arguments: Some(vec![PromptArgument {
                            name: "city".to_string(),
                            description: Some("City name".to_string()),
                            required: Some(true),
                        }]),
                    },
                    Prompt {
                        name: "[mock data]analyze-code".to_string(),
                        description: Some(
                            "[mock data]Analyze code for potential improvements".to_string(),
                        ),
                        arguments: Some(vec![PromptArgument {
                            name: "language".to_string(),
                            description: Some("Programming language".to_string()),
                            required: Some(true),
                        }]),
                    },
                ],
            };

            let res = JSONRPCResponse::new(request_id, serde_json::to_value(result).unwrap());

            let event =
                SseEvent::new_event(session_id, "message", &serde_json::to_string(&res).unwrap());
            let _ = mcp_proxy.tx.send(event);
            mcp_proxy.response_accepted(session).await?;
            return Ok(true);
        }
        "prompts/get" => {
            // let res = JSONRPCResponse::new(
            //     request_id,
            //     serde_json::to_value(result).unwrap(),
            // );
            mcp_proxy.response_accepted(session).await?;
            return Ok(true);
        }
        _ => {
            let _ = mcp_proxy.tx.send(SseEvent::new(session_id, "Accepted"));
            mcp_proxy.response_accepted(session).await?;
            return Ok(true);
        }
    }
    Ok(false)
}
