use std::collections::HashMap;

use pingora::Result;
use pingora_proxy::Session;
use serde_json::Map;

use crate::{
    config::{SERVER_NAME, SERVER_VERSION},
    jsonrpc::{JSONRPCRequest, JSONRPCResponse, LATEST_PROTOCOL_VERSION},
    mcp::response_sender::ResponseSender,
    service::mcp::MCPProxyService,
    types::{Implementation, InitializeResult, ServerCapabilities, ServerCapabilitiesTools},
};

/// Handler for the 'initialize' MCP method
pub struct InitializeHandler;

impl InitializeHandler {
    /// Handles the initialize request
    ///
    /// # Arguments
    /// * `mcp_proxy` - MCP proxy service instance
    /// * `session` - HTTP session
    /// * `request` - JSON-RPC request
    /// * `stream` - Whether to use SSE (true) or HTTP (false)
    /// * `session_id` - SSE session identifier
    pub async fn handle(
        mcp_proxy: &MCPProxyService,
        session: &mut Session,
        request: &JSONRPCRequest,
        stream: bool,
        session_id: &str,
    ) -> Result<bool> {
        log::info!("Handling initialize request");

        let result = Self::build_initialize_result();
        let res = JSONRPCResponse::new(
            request.id.clone().unwrap(),
            serde_json::to_value(result).unwrap(),
        );

        ResponseSender::send(mcp_proxy, session, &res, stream, session_id).await?;
        Ok(true)
    }

    /// Builds the InitializeResult response
    fn build_initialize_result() -> InitializeResult {
        InitializeResult {
            meta: Map::new(),
            protocol_version: LATEST_PROTOCOL_VERSION.to_string(),
            capabilities: ServerCapabilities {
                completions: Map::new(),
                experimental: HashMap::new(),
                logging: Map::new(),
                prompts: None,
                resources: None,
                tools: Some(ServerCapabilitiesTools { list_changed: None }),
            },
            server_info: Implementation {
                name: SERVER_NAME.to_string(),
                version: SERVER_VERSION.to_string(),
            },
            instructions: None,
        }
    }
}