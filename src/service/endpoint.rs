use crate::{mcp, service::mcp::MCPProxyService, utils};
use pingora_error::Result;
use pingora_proxy::{ProxyHttp, Session};

/// Constants for MCP protocol handling
pub const MCP_STREAMABLE_HTTP: &str = "streamable_http";
pub const MCP_SESSION_ID: &str = "mcp-session-id";
pub const MCP_REQUEST_ID: &str = "mcp-request-id";

/// Streamable HTTP types
const STREAM_TYPE: &str = "stream";
const STATELESS_TYPE: &str = "stateless";
/// Handles streamable HTTP endpoint requests (both GET and POST methods)
/// GET: Establishes SSE stream connection
/// POST: Processes initialization or resuming of a stream
pub async fn handle_streamable_http_endpoint(
    ctx: &mut <MCPProxyService as ProxyHttp>::CTX,
    mcp_proxy: &MCPProxyService,
    session: &mut Session,
) -> Result<bool> {
    match session.req_header().method {
        http::Method::GET => handle_get_request(ctx, mcp_proxy, session).await,
        http::Method::POST => handle_post_request(ctx, mcp_proxy, session).await,
        _ => Ok(false),
    }
}

/// Handles GET requests for SSE stream establishment
async fn handle_get_request(
    ctx: &mut <MCPProxyService as ProxyHttp>::CTX,
    mcp_proxy: &MCPProxyService,
    session: &mut Session,
) -> Result<bool> {
    ctx.vars
        .insert(MCP_STREAMABLE_HTTP.to_string(), STREAM_TYPE.to_string());

    log::debug!("Handle GET requests for SSE streams (using built-in support from StreamableHTTP)");

    let last_event_id = session.req_header().headers.get("last-event-id");
    if let Some(last_event_id) = last_event_id {
        log::info!("Client reconnecting with Last-Event-ID: {last_event_id:?}");
    } else {
        log::info!("Establishing new SSE stream");
    }

    mcp_proxy.response_sse(session).await
}

/// Handles POST requests for initialization or resuming streams
async fn handle_post_request(
    ctx: &mut <MCPProxyService as ProxyHttp>::CTX,
    mcp_proxy: &MCPProxyService,
    session: &mut Session,
) -> Result<bool> {
    ctx.vars
        .insert(MCP_STREAMABLE_HTTP.to_string(), STATELESS_TYPE.to_string());

    log::debug!("Handle POST requests for initialization or resuming a stream");

    // Check if reusing existing transport
    if session.req_header().headers.get(MCP_SESSION_ID).is_some() {
        log::debug!("Reuse existing transport");
        return Ok(false);
    }

    // Parse and process JSON-RPC request
    match mcp_proxy.parse_json_rpc_request(session).await {
        Ok(request) => {
            if let Some(id) = &request.id {
                ctx.vars.insert(MCP_REQUEST_ID.to_string(), id.to_string());
            }
            mcp::request_processing_streamable_http(ctx, "session_id", mcp_proxy, session, &request)
                .await
        }
        Err(e) => {
            log::error!("Failed to process JSON-RPC request: {e}");
            Ok(false)
        }
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
    let request = match mcp_proxy.parse_json_rpc_request(session).await {
        Ok(req) => req,
        Err(e) => {
            log::error!("Failed to parse JSON: {e}");
            return Ok(false);
        }
    };

    // Extract session_id from query parameters
    let session_id = match extract_session_id(session) {
        Some(id) => id,
        None => {
            log::error!("'session_id' query parameter is missing");
            return Ok(false);
        }
    };

    log::info!("session_id: {session_id}");

    // Store session and request IDs in context
    ctx.vars
        .insert(MCP_SESSION_ID.to_string(), session_id.clone());

    if let Some(id) = &request.id {
        ctx.vars.insert(MCP_REQUEST_ID.to_string(), id.to_string());
    }

    mcp::request_processing(ctx, &session_id, mcp_proxy, session, &request).await
}

/// Extracts session_id from query parameters
fn extract_session_id(session: &Session) -> Option<String> {
    utils::request::query_to_map(&session.req_header().uri)
        .get("session_id")
        .map(|s| s.to_string())
}
