use pingora::Result;
use pingora_proxy::Session;

use crate::{
    config::global_mcp_route_meta_info_fetch,
    jsonrpc::{CallToolRequestParam, JSONRPCResponse, RequestId},
    mcp::{request_builder::RequestBuilder, result_builder::ResultBuilder, send_json_response},
    proxy::{mcp::mcp_service_fetch, ProxyContext},
    service::mcp::MCPProxyService,
};

/// Handler for tools/call requests
pub struct ToolCallHandler;

impl ToolCallHandler {
    /// Handles the tools/call request
    ///
    /// # Arguments
    /// * `ctx` - Proxy context
    /// * `tenant_id` - Optional tenant ID
    /// * `request_id` - JSON-RPC request ID
    /// * `params` - Tool call parameters
    /// * `mcp_proxy` - MCP proxy service
    /// * `session` - HTTP session
    /// * `stream` - Whether to use streaming response
    /// * `session_id` - SSE session ID
    pub async fn handle(
        ctx: &mut ProxyContext,
        tenant_id: Option<&str>,
        request_id: RequestId,
        params: Option<serde_json::Value>,
        mcp_proxy: &MCPProxyService,
        session: &mut Session,
        stream: bool,
        session_id: &str,
    ) -> Result<bool> {
        // Validate parameters exist
        let params = match params {
            Some(p) => p,
            None => {
                return Self::send_error_response(
                    mcp_proxy,
                    session,
                    request_id,
                    ResultBuilder::missing_params(),
                    stream,
                    session_id,
                )
                .await;
            }
        };

        // Parse tool call parameters
        let tool_params: CallToolRequestParam = match serde_json::from_value(params) {
            Ok(p) => p,
            Err(e) => {
                log::error!("Failed to deserialize CallToolRequestParam: {e}");
                return Self::send_error_response(
                    mcp_proxy,
                    session,
                    request_id,
                    ResultBuilder::invalid_params(),
                    stream,
                    session_id,
                )
                .await;
            }
        };

        log::debug!("Tool call parameters: {tool_params:?}");

        // Find route metadata for the tool
        let route_meta_info = Self::find_route_meta(tenant_id, &tool_params.name);

        match route_meta_info {
            Some(route_info) => {
                // Extract arguments
                let arguments = match &tool_params.arguments {
                    Some(args) => args,
                    None => {
                        log::warn!("No arguments provided for tool: {}", tool_params.name);
                        return Ok(false);
                    }
                };

                log::debug!("Route metadata: {route_info:#?}");

                // Build upstream request
                RequestBuilder::build_request(ctx, session, route_info, arguments);

                // Return false to continue proxying to upstream
                Ok(false)
            }
            None => {
                log::warn!("Tool not found: {}", tool_params.name);
                Self::send_error_response(
                    mcp_proxy,
                    session,
                    RequestId::from(0),
                    ResultBuilder::tool_not_found(&tool_params.name),
                    stream,
                    session_id,
                )
                .await
            }
        }
    }

    /// Finds route metadata for a tool
    fn find_route_meta(
        tenant_id: Option<&str>,
        tool_name: &str,
    ) -> Option<std::sync::Arc<crate::config::MCPRouteMetaInfo>> {
        match tenant_id {
            Some(id) => {
                log::debug!("Finding tool route for tenant: {id}");
                mcp_service_fetch(id)
                    .and_then(|service| service.get_meta_info(tool_name))
            }
            None => {
                log::debug!("Finding global tool route");
                global_mcp_route_meta_info_fetch(tool_name)
            }
        }
    }

    /// Sends an error response
    async fn send_error_response(
        mcp_proxy: &MCPProxyService,
        session: &mut Session,
        request_id: RequestId,
        result: crate::types::CallToolResult,
        stream: bool,
        session_id: &str,
    ) -> Result<bool> {
        let res = JSONRPCResponse::new(request_id, serde_json::to_value(result).unwrap());
        send_json_response(mcp_proxy, session, &res, stream, session_id).await?;
        Ok(true)
    }
}