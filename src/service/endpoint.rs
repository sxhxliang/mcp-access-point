use crate::{mcp, service::mcp::MCPProxyService, utils};
use pingora::Result;
use pingora_proxy::{ProxyHttp, Session};

/// Constant for identifying streamable HTTP requests in context variables
pub const MCP_STREAMABLE_HTTP: &str = "streamable_http";
/// Constant for storing session ID in HTTP headers
pub const MCP_SESSION_ID: &str = "mcp-session-id";
/// Constant for storing request ID in context variables
pub const MCP_REQUEST_ID: &str = "mcp-request-id";
/// Handles streamable HTTP endpoint requests (both GET and POST methods)
/// GET: Establishes SSE stream connection
/// POST: Processes initialization or resuming of a stream
pub async fn handle_streamable_http_endpoint(
    ctx: &mut <MCPProxyService as ProxyHttp>::CTX,
    mcp_proxy: &MCPProxyService,
    session: &mut Session,
) -> Result<bool> {
    let mcp_session_id = session.req_header().headers.get(MCP_SESSION_ID);

    match session.req_header().method {
        http::Method::GET => {
            ctx.vars
                .insert(MCP_STREAMABLE_HTTP.to_string(), "stream".to_string());
            log::debug!(
                "Handle GET requests for SSE streams (using built-in support from StreamableHTTP)"
            );

            let last_event_id = session.req_header().headers.get("last-event-id");
            log::debug!("req_header: {:?}", session.req_header());

            if let Some(last_event_id) = last_event_id {
                log::info!(
                    "Client reconnecting with Last-Event-ID: {last_event_id:?}"
                );
            } else {
                log::info!(
                    "Establishing new SSE stream for session {mcp_session_id:?}"
                );
            }
            mcp_proxy.response_sse(session).await
        }
        http::Method::POST => {
            // add var to ctx
            ctx.vars
                .insert(MCP_STREAMABLE_HTTP.to_string(), "stateless".to_string());

            log::debug!("Handle POST requests for initialization or resuming a stream");

            if let Some(_mcp_session_id) = mcp_session_id {
                log::debug!("Reuse existing transport");
                Ok(false)
            } else {
                match mcp_proxy.parse_json_rpc_request(session).await {
                    Ok(request) => {
                        // add vars to ctx
                        if let Some(id) = &request.id {
                            ctx.vars
                                .insert(MCP_REQUEST_ID.to_string(), id.to_string());
                        }
                        mcp::request_processing_streamable_http(
                            ctx,
                            "session_id",
                            mcp_proxy,
                            session,
                            &request,
                        )
                        .await
                    }
                    Err(e) => {
                        log::error!("Failed to process JSON-RPC request: {e}");
                        Ok(false)
                    }
                }
            }
        }
        _ => Ok(false),
    }
}

/// Handles SSE endpoint requests by delegating to MCPProxyService's SSE response handler
pub async fn handle_sse_endpoint(
    _ctx: &mut <MCPProxyService as ProxyHttp>::CTX,
    mcp_proxy: &MCPProxyService,
    session: &mut Session,
) -> Result<bool> {
    mcp_proxy.response_sse(session).await
}

/// Handles message endpoint requests by parsing JSON-RPC and processing accordingly
pub async fn handle_message_endpoint(
    ctx: &mut <MCPProxyService as ProxyHttp>::CTX,
    mcp_proxy: &MCPProxyService,
    session: &mut Session,
) -> Result<bool> {
    match mcp_proxy.parse_json_rpc_request(session).await {
        Ok(request) => {
            let session_id = match utils::request::query_to_map(&session.req_header().uri)
                .get("session_id")
                .map(|s| s.to_string())
            {
                Some(id) => id,
                None => {
                    log::error!("'session_id' query parameter is missing");
                    // 理想情况下，这里应该发送一个 400 Bad Request 响应。
                    // 由于函数签名限制，我们先记录日志并停止处理。
                    return Ok(false);
                }
            };
            log::info!("session_id: {session_id}");

            // add vars to ctx
            ctx.vars
                .insert(MCP_SESSION_ID.to_string(), session_id.clone());

            if let Some(id) = &request.id {
                ctx.vars.insert(MCP_REQUEST_ID.to_string(), id.to_string());
            }

            mcp::request_processing(ctx, &session_id, mcp_proxy, session, &request).await
        }
        Err(e) => {
            log::error!("Failed to parse JSON: {e}");
            Ok(false)
        }
    }
}
