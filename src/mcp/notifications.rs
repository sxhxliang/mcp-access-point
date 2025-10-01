use pingora::Result;
use pingora_proxy::{ProxyHttp, Session};

/// Handler for completion requests
pub struct CompletionHandler;
use crate::{
    jsonrpc::JSONRPCRequest,
    mcp::ping_handler::PingHandler,
    service::mcp::MCPProxyService, sse_event::SseEvent,
};

/// Processes notification-related MCP requests
///
/// # Arguments
/// * `ctx` - Proxy context for storing route and request information
/// * `session_id` - SSE session identifier
/// * `mcp_proxy` - MCP proxy service instance
/// * `session` - HTTP session
/// * `request` - JSON-RPC request to process
/// * `stream` - Whether to use streaming response (SSE)
pub async fn request_processing(
    _ctx: &mut <MCPProxyService as ProxyHttp>::CTX,
    session_id: &str,
    mcp_proxy: &MCPProxyService,
    session: &mut Session,
    request: &JSONRPCRequest,
    stream: bool,
) -> Result<bool> {
    match request.method.as_str() {
        "ping" => PingHandler::handle(mcp_proxy, session, stream, session_id).await,

        "notifications/initialized" | "notifications/cancelled" => {
            NotificationHandler::handle_initialized_or_cancelled(
                mcp_proxy, session, stream, session_id,
            )
            .await
        }

        "notifications/roots/list_changed" => {
            NotificationHandler::handle_roots_list_changed(mcp_proxy, session, stream).await
        }

        "completion/complete" => CompletionHandler::handle(mcp_proxy, session, stream).await,

        _ => {
            log::warn!("Unknown notification method: {}", request.method);
            Ok(true)
        }
    }
}

pub struct NotificationHandler;

impl NotificationHandler {
    /// Handles initialized and cancelled notifications
    ///
    /// # Arguments
    /// * `mcp_proxy` - MCP proxy service instance
    /// * `session` - HTTP session
    /// * `stream` - Whether to use SSE (true) or HTTP (false)
    /// * `session_id` - SSE session identifier
    pub async fn handle_initialized_or_cancelled(
        mcp_proxy: &MCPProxyService,
        session: &mut Session,
        stream: bool,
        session_id: &str,
    ) -> Result<bool> {
        log::debug!("Handling initialized/cancelled notification");

        if stream {
            let _ = mcp_proxy.event_sender().send(SseEvent::new(session_id, "Accepted"));
            mcp_proxy.response_accepted(session).await?;
        }

        Ok(true)
    }

    /// Handles roots list changed notification
    ///
    /// # Arguments
    /// * `mcp_proxy` - MCP proxy service instance
    /// * `session` - HTTP session
    /// * `stream` - Whether to use SSE (true) or HTTP (false)
    pub async fn handle_roots_list_changed(
        mcp_proxy: &MCPProxyService,
        session: &mut Session,
        stream: bool,
    ) -> Result<bool> {
        log::debug!("Handling roots/list_changed notification");

        if stream {
            mcp_proxy.response_accepted(session).await?;
        }

        Ok(true)
    }
}


impl CompletionHandler {
    /// Handles completion/complete requests
    ///
    /// # Arguments
    /// * `mcp_proxy` - MCP proxy service instance
    /// * `session` - HTTP session
    /// * `stream` - Whether to use SSE (true) or HTTP (false)
    pub async fn handle(
        mcp_proxy: &MCPProxyService,
        session: &mut Session,
        stream: bool,
    ) -> Result<bool> {
        log::debug!("Handling completion/complete request");

        // TODO: Implement resource completion logic

        if stream {
            mcp_proxy.response_accepted(session).await?;
        }

        Ok(true)
    }
}
