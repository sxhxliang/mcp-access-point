use pingora::Result;
use pingora_proxy::{ProxyHttp, Session};

use crate::{
    jsonrpc::{JSONRPCRequest, JSONRPCResponse, RequestId},
    mcp::{
        send_json_response, tool_call_handler::ToolCallHandler, tool_list_handler::ToolListHandler,
    },
    service::mcp::MCPProxyService,
};

/// Processes tool-related JSON-RPC requests (tools/list and tools/call)
///
/// # Arguments
/// * `ctx` - Proxy context for storing route and request information
/// * `session_id` - SSE session identifier
/// * `mcp_proxy` - MCP proxy service instance
/// * `session` - HTTP session
/// * `request` - JSON-RPC request to process
/// * `stream` - Whether to use streaming response (SSE)
pub async fn request_processing(
    ctx: &mut <MCPProxyService as ProxyHttp>::CTX,
    session_id: &str,
    mcp_proxy: &MCPProxyService,
    session: &mut Session,
    request: &JSONRPCRequest,
    stream: bool,
) -> Result<bool> {
    let request_id = request.id.clone().unwrap_or(RequestId::Integer(0));
    let tenant_id = ctx.vars.get("MCP_TENANT_ID").cloned();

    match request.method.as_str() {
        "tools/list" => {
            ToolListHandler::handle(
                tenant_id.as_deref(),
                request_id,
                mcp_proxy,
                session,
                stream,
                session_id,
            )
            .await
        }
        "tools/call" => {
            log::debug!("Handling tools/call request");
            ToolCallHandler::handle(
                ctx,
                tenant_id.as_deref(),
                request_id,
                request.params.clone(),
                mcp_proxy,
                session,
                stream,
                session_id,
            )
            .await
        }
        _ => {
            // Unknown method - send empty response
            let res = JSONRPCResponse::new(request_id, serde_json::to_value("{}").unwrap());
            send_json_response(mcp_proxy, session, &res, stream, session_id).await?;
            Ok(true)
        }
    }
}
