
mod initialize_handler;
mod method_dispatcher;
mod notifications;
mod ping_handler;
mod prompts;
mod request_builder;
mod response_sender;
mod resources;
mod result_builder;
mod sampling;
mod tool_call_handler;
mod tool_list_handler;
mod tools;

use crate::{
    jsonrpc::JSONRPCRequest,
    mcp::{initialize_handler::InitializeHandler, method_dispatcher::MethodDispatcher},
    service::mcp::MCPProxyService,
};

use pingora::Result;
use pingora_proxy::{ProxyHttp, Session};

// Re-export for backward compatibility
pub use response_sender::ResponseSender;

pub async fn send_json_response(
    mcp_proxy: &MCPProxyService,
    session: &mut Session,
    res: &crate::jsonrpc::JSONRPCResponse,
    stream: bool,
    session_id: &str,
) -> Result<()> {
    ResponseSender::send(mcp_proxy, session, res, stream, session_id).await
}

/// SSE protocol handler (2024-11-05 specification)
pub async fn request_processing(
    ctx: &mut <MCPProxyService as ProxyHttp>::CTX,
    session_id: &str,
    mcp_proxy: &MCPProxyService,
    session: &mut Session,
    request: &JSONRPCRequest,
) -> Result<bool> {
    match request.method.as_str() {
        "initialize" => {
            InitializeHandler::handle(mcp_proxy, session, request, true, session_id).await
        }
        _ => {
            MethodDispatcher::dispatch(ctx, session_id, mcp_proxy, session, request, true).await
        }
    }
}

/// Streamable HTTP protocol handler (2025-03-26 specification)
pub async fn request_processing_streamable_http(
    ctx: &mut <MCPProxyService as ProxyHttp>::CTX,
    session_id: &str,
    mcp_proxy: &MCPProxyService,
    session: &mut Session,
    request: &JSONRPCRequest,
) -> Result<bool> {
    log::debug!("using request: {request:#?}");
    match request.method.as_str() {
        "initialize" => {
            InitializeHandler::handle(mcp_proxy, session, request, false, session_id).await
        }
        _ => {
            MethodDispatcher::dispatch(ctx, session_id, mcp_proxy, session, request, false).await
        }
    }
}
