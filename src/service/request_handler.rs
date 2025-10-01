use http::StatusCode;
use pingora_error::Result;
use pingora_proxy::Session;

use crate::{
    config::{CLIENT_MESSAGE_ENDPOINT, CLIENT_SSE_ENDPOINT, CLIENT_STREAMABLE_HTTP_ENDPOINT},
    proxy::ProxyContext,
    service::{endpoint, mcp::MCPProxyService},
    utils::request::{match_api_path, PathMatch},
};

/// Request routing and path matching handler
pub struct RequestHandler;

impl RequestHandler {
    /// Checks if the path is a known MCP endpoint
    pub fn is_known_endpoint(path: &str) -> bool {
        path == CLIENT_SSE_ENDPOINT
            || path == CLIENT_MESSAGE_ENDPOINT
            || path == CLIENT_STREAMABLE_HTTP_ENDPOINT
            || match_api_path(path) != PathMatch::NoMatch
    }

    /// Handles tenant-specific path configuration
    pub fn handle_tenant_path(tenant_id: String, ctx: &mut ProxyContext, session: &mut Session) {
        ctx.vars
            .insert("MCP_TENANT_ID".to_string(), tenant_id.clone());
        let _ = session
            .req_header_mut()
            .insert_header("MCP_TENANT_ID", tenant_id);
    }

    /// Routes request to appropriate endpoint handler based on path
    pub async fn route_request(
        path: &str,
        ctx: &mut ProxyContext,
        mcp_proxy: &MCPProxyService,
        session: &mut Session,
    ) -> Result<Option<bool>> {
        match match_api_path(path) {
            PathMatch::Sse(tenant_id) => {
                log::debug!("SSE path: {path:?}");
                Self::handle_tenant_path(tenant_id, ctx, session);
                Ok(Some(
                    endpoint::handle_sse_endpoint(ctx, mcp_proxy, session).await?,
                ))
            }
            PathMatch::Messages(tenant_id) => {
                log::debug!("Messages path: {path:?}");
                Self::handle_tenant_path(tenant_id, ctx, session);
                Ok(Some(
                    endpoint::handle_message_endpoint(ctx, mcp_proxy, session).await?,
                ))
            }
            PathMatch::StreamableHttp(tenant_id) => {
                log::debug!("Streamable HTTP path: {path:?}");
                Self::handle_tenant_path(tenant_id, ctx, session);
                Ok(Some(
                    endpoint::handle_streamable_http_endpoint(ctx, mcp_proxy, session).await?,
                ))
            }
            PathMatch::NoMatch => {
                log::debug!("No tenant match for path: {path:?}, using global mcp endpoint.");
                Self::route_global_endpoint(path, ctx, mcp_proxy, session).await
            }
        }
    }

    /// Routes request to global (non-tenant) endpoint handlers
    async fn route_global_endpoint(
        path: &str,
        ctx: &mut ProxyContext,
        mcp_proxy: &MCPProxyService,
        session: &mut Session,
    ) -> Result<Option<bool>> {
        match path {
            CLIENT_STREAMABLE_HTTP_ENDPOINT => {
                // 2025-03-26 specification protocol
                Ok(Some(
                    endpoint::handle_streamable_http_endpoint(ctx, mcp_proxy, session).await?,
                ))
            }
            CLIENT_SSE_ENDPOINT => {
                // 2024-11-05 specification protocol
                Ok(Some(
                    endpoint::handle_sse_endpoint(ctx, mcp_proxy, session).await?,
                ))
            }
            CLIENT_MESSAGE_ENDPOINT => {
                // 2024-11-05 specification protocol
                Ok(Some(
                    endpoint::handle_message_endpoint(ctx, mcp_proxy, session).await?,
                ))
            }
            _ => Ok(None),
        }
    }

    /// Handles unknown route with 404 response
    pub async fn handle_unknown_route(session: &mut Session) -> Result<bool> {
        let path = session.req_header().uri.path();
        log::warn!("Route not found for path: {path}");
        session
            .respond_error(StatusCode::NOT_FOUND.as_u16())
            .await?;
        Ok(true)
    }
}
