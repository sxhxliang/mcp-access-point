use pingora::{proxy::Session, Result};

use crate::{proxy::ModelContextProtocolProxy, sse_event::SseEvent, types::JSONRPCRequest};

// Helper function to send an SseEvent and mark the response as accepted
async fn process_response(
    session_id: &str,
    event_message: &str,
    mcp_proxy: &ModelContextProtocolProxy,
    session: &mut Session,
) -> Result<()> {
    let _ = mcp_proxy.tx.send(SseEvent::new(session_id, event_message));
    mcp_proxy.response_accepted(session).await?;
    Ok(())
}

pub async fn request_processing(
    session_id: &str,
    mcp_proxy: &ModelContextProtocolProxy,
    session: &mut Session,
    request: &JSONRPCRequest,
) -> Result<bool> {
    // Safely handle the request ID assignment
    let request_id = request.id.unwrap_or(0);

    match request.method.as_str() {
        "ping" => {
            log::debug!("ping...");
            process_response(session_id, "{}", mcp_proxy, session).await?;
            Ok(true)
        }
        "notifications/initialized" | "notifications/cancelled" => {
            log::debug!("notifications/initialized or notifications/cancelled");
            process_response(session_id, "Accepted", mcp_proxy, session).await?;
            Ok(true)
        }
        "notifications/roots/list_changed" => {
            log::debug!("notifications/roots/list_changed");
            mcp_proxy.response_accepted(session).await?;
            Ok(true)
        }
        "completion/complete" => {
            log::debug!("completion/complete");
            // TODO: Implement resource completion logic
            mcp_proxy.response_accepted(session).await?;
            Ok(true)
        }
        _ => {
            process_response(session_id, "Accepted", mcp_proxy, session).await?;
            Ok(true)
        }
    }
}
