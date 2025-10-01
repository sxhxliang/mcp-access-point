use pingora::Result;
use pingora_proxy::Session;

use crate::{
    jsonrpc::{JSONRPCResponse, RequestId},
    mcp::send_json_response,
    proxy::mcp::{global_openapi_tools_fetch, mcp_service_fetch},
    service::mcp::MCPProxyService,
    types::ListToolsResult,
};

/// Handler for tools/list requests
pub struct ToolListHandler;

impl ToolListHandler {
    /// Handles the tools/list request
    ///
    /// # Arguments
    /// * `tenant_id` - Optional tenant ID for multi-tenancy support
    /// * `request_id` - JSON-RPC request ID
    /// * `mcp_proxy` - MCP proxy service instance
    /// * `session` - HTTP session
    /// * `stream` - Whether to use streaming response
    /// * `session_id` - SSE session ID
    pub async fn handle(
        tenant_id: Option<&str>,
        request_id: RequestId,
        mcp_proxy: &MCPProxyService,
        session: &mut Session,
        stream: bool,
        session_id: &str,
    ) -> Result<bool> {
        let list_tools = Self::fetch_tools(tenant_id);

        match list_tools {
            Some(tools) => {
                let res = JSONRPCResponse::new(request_id, serde_json::to_value(tools).unwrap());
                send_json_response(mcp_proxy, session, &res, stream, session_id).await?;
                Ok(true)
            }
            None => {
                log::warn!("No tools found");
                Ok(false)
            }
        }
    }

    /// Fetches tools based on tenant ID
    fn fetch_tools(tenant_id: Option<&str>) -> Option<ListToolsResult> {
        match tenant_id {
            Some(id) => {
                log::debug!("Fetching tools for tenant: {id}");
                mcp_service_fetch(id)
                    .and_then(|service| service.get_tools())
                    .or_else(|| Some(ListToolsResult::default()))
            }
            None => {
                log::debug!("Fetching global tools");
                global_openapi_tools_fetch()
            }
        }
    }
}