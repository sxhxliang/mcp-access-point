use pingora::Result;
use pingora_proxy::{ProxyHttp, Session};

use crate::{
    jsonrpc::JSONRPCRequest,
    mcp::{notifications, prompts, resources, tools},
    service::mcp::MCPProxyService,
};

/// Dispatches MCP method requests to appropriate handlers
pub struct MethodDispatcher;

impl MethodDispatcher {
    /// Routes a JSON-RPC request to the appropriate handler based on method name
    ///
    /// # Arguments
    /// * `ctx` - Proxy context for storing route and request information
    /// * `session_id` - SSE session identifier
    /// * `mcp_proxy` - MCP proxy service instance
    /// * `session` - HTTP session
    /// * `request` - JSON-RPC request to process
    /// * `stream` - Whether to use streaming response (SSE)
    pub async fn dispatch(
        ctx: &mut <MCPProxyService as ProxyHttp>::CTX,
        session_id: &str,
        mcp_proxy: &MCPProxyService,
        session: &mut Session,
        request: &JSONRPCRequest,
        stream: bool,
    ) -> Result<bool> {
        match request.method.as_str() {
            "ping"
            | "notifications/initialized"
            | "notifications/cancelled"
            | "notifications/roots/list_changed"
            | "completion/complete" => {
                notifications::request_processing(ctx, session_id, mcp_proxy, session, request, stream)
                    .await
            }

            "tools/list" | "tools/call" => {
                tools::request_processing(ctx, session_id, mcp_proxy, session, request, stream)
                    .await
            }

            "resources/list" | "resources/read" | "resources/templates/list" => {
                resources::request_processing(ctx, session_id, mcp_proxy, session, request, stream)
                    .await
            }

            "prompts/list" | "prompts/get" => {
                prompts::request_processing(ctx, session_id, mcp_proxy, session, request, stream)
                    .await
            }

            _ => {
                log::info!("Unknown method called: {}", request.method);
                Ok(false) // Gracefully handle unknown methods
            }
        }
    }
}
