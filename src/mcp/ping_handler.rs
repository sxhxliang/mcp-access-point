use pingora::Result;
use pingora_proxy::Session;

use crate::{service::mcp::MCPProxyService, sse_event::SseEvent};

/// Handler for ping requests
pub struct PingHandler;

impl PingHandler {
    /// Handles ping requests
    ///
    /// # Arguments
    /// * `mcp_proxy` - MCP proxy service instance
    /// * `session` - HTTP session
    /// * `stream` - Whether to use SSE (true) or HTTP (false)
    /// * `session_id` - SSE session identifier
    pub async fn handle(
        mcp_proxy: &MCPProxyService,
        session: &mut Session,
        stream: bool,
        session_id: &str,
    ) -> Result<bool> {
        log::debug!("Handling ping request");

        if stream {
            let _ = mcp_proxy.event_sender().send(SseEvent::new(session_id, "{}"));
            mcp_proxy.response_accepted(session).await?;
        }

        Ok(true)
    }
}
