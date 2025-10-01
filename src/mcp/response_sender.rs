use http::StatusCode;
use pingora::Result;
use pingora_proxy::Session;

use crate::{jsonrpc::JSONRPCResponse, service::mcp::MCPProxyService, sse_event::SseEvent};

/// Handles sending JSON-RPC responses via SSE or HTTP
pub struct ResponseSender;

impl ResponseSender {
    /// Sends a JSON-RPC response using SSE (Server-Sent Events)
    ///
    /// # Arguments
    /// * `mcp_proxy` - MCP proxy service instance
    /// * `session` - HTTP session
    /// * `res` - JSON-RPC response to send
    /// * `session_id` - SSE session identifier
    pub async fn send_sse(
        mcp_proxy: &MCPProxyService,
        session: &mut Session,
        res: &JSONRPCResponse,
        session_id: &str,
    ) -> Result<()> {
        let event =
            SseEvent::new_event(session_id, "message", &serde_json::to_string(res).unwrap());
        let _ = mcp_proxy.event_sender().send(event);
        mcp_proxy.response_accepted(session).await
    }

    /// Sends a JSON-RPC response via HTTP
    ///
    /// # Arguments
    /// * `mcp_proxy` - MCP proxy service instance
    /// * `session` - HTTP session
    /// * `res` - JSON-RPC response to send
    pub async fn send_http(
        mcp_proxy: &MCPProxyService,
        session: &mut Session,
        res: &JSONRPCResponse,
    ) -> Result<()> {
        mcp_proxy
            .response(
                session,
                StatusCode::OK,
                serde_json::to_string(res).unwrap(),
            )
            .await?;
        Ok(())
    }

    /// Sends a JSON-RPC response using the appropriate transport
    ///
    /// # Arguments
    /// * `mcp_proxy` - MCP proxy service instance
    /// * `session` - HTTP session
    /// * `res` - JSON-RPC response to send
    /// * `stream` - Whether to use SSE (true) or HTTP (false)
    /// * `session_id` - SSE session identifier (required if stream is true)
    pub async fn send(
        mcp_proxy: &MCPProxyService,
        session: &mut Session,
        res: &JSONRPCResponse,
        stream: bool,
        session_id: &str,
    ) -> Result<()> {
        if stream {
            Self::send_sse(mcp_proxy, session, res, session_id).await
        } else {
            Self::send_http(mcp_proxy, session, res).await
        }
    }
}