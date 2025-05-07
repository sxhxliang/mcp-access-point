use pingora::{proxy::Session, Result};
use pingora_proxy::ProxyHttp;

use crate::{
    service::mcp::MCPProxyService,
    sse_event::SseEvent,
    types::RequestId,
    jsonrpc::JSONRPCRequest
};

// Helper function to send an SseEvent and mark the response as accepted
async fn process_response(
    ctx: &mut <MCPProxyService as ProxyHttp>::CTX,
    session_id: &str,
    event_message: &str,
    mcp_proxy: &MCPProxyService,
    session: &mut Session,
) -> Result<()> {
    let _ = mcp_proxy.tx.send(SseEvent::new(session_id, event_message));
    mcp_proxy.response_accepted(session).await?;
    Ok(())
}

pub async fn request_processing(
    ctx: &mut <MCPProxyService as ProxyHttp>::CTX,
    session_id: &str,
    mcp_proxy: &MCPProxyService,
    session: &mut Session,
    request: &JSONRPCRequest,
    stream: bool, // TODO: Implement stream handling if needed, currently unused in this cod
) -> Result<bool> {
    // Safely handle the request ID assignment
    let request_id = request.id.clone().unwrap_or(RequestId::Integer(0));

    match request.method.as_str() {
        "ping" => {
            log::debug!("ping...");
            if stream {
                process_response(ctx, session_id, "{}", mcp_proxy, session).await?;
            }
            Ok(true)
        }
        "notifications/initialized" | "notifications/cancelled" => {
            log::debug!("notifications/initialized or notifications/cancelled");
            if stream {
                process_response(ctx, session_id, "Accepted", mcp_proxy, session).await?;
            }
            Ok(true)
        }
        "notifications/roots/list_changed" => {
            log::debug!("notifications/roots/list_changed");
            if stream {
                mcp_proxy.response_accepted(session).await?;
            }
            Ok(true)
        }
        "completion/complete" => {
            log::debug!("completion/complete");
            // TODO: Implement resource completion logic
            if stream {
                mcp_proxy.response_accepted(session).await?;
            }
            Ok(true)
        }
        _ => {
            if stream {
                process_response(ctx, session_id, "Accepted", mcp_proxy, session).await?;
            }
            Ok(true)
        }
    }
}
