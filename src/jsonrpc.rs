use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::borrow::Cow;

use crate::{
    config::ERROR_MESSAGE,
    types::{CallToolResult, CallToolResultContentItem, RequestId, TextContent},
};
use pingora::{Error, ErrorType, Result};
pub const LATEST_PROTOCOL_VERSION: &str = "2024-11-05";
pub const JSONRPC_VERSION: &str = "2.0";

pub const PARSE_ERROR: i32 = -32700;
pub const INVALID_REQUEST: i32 = -32600;
pub const METHOD_NOT_FOUND: i32 = -32601;
pub const INVALID_PARAMS: i32 = -32602;
pub const INTERNAL_ERROR: i32 = -32603;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorCode {
    // Standard JSON-RPC error codes
    ParseError = -32700,
    InvalidRequest = -32600,
    MethodNotFound = -32601,
    InvalidParams = -32602,
    InternalError = -32603,
    // SDKs and applications can define their own error codes above -32000.
    OwnErrorCode = -32000,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ProgressToken {
    String(String),
    Number(i64),
}

// JSON-RPC
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "jsonrpc", content = "content")]
pub enum JSONRPCMessage {
    // #[serde(rename = "2.0")]
    Request(JSONRPCRequest),
    // #[serde(rename = "2.0")]
    Notification(JSONRPCNotification),
    // #[serde(rename = "2.0")]
    Response(JSONRPCResponse),
    // #[serde(rename = "2.0")]
    Error(JSONRPCError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JSONRPCRequest {
    pub jsonrpc: String,
    pub id: Option<RequestId>,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JSONRPCNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// JSON-RPC 2.0
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JSONRPCResponse {
    #[serde(default = "default_jsonrpc_version")]
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<RequestId>,

    pub result: Value,
}

/// jsonrpc default "2.0"
fn default_jsonrpc_version() -> String {
    JSONRPC_VERSION.to_string()
}

impl Default for JSONRPCResponse {
    fn default() -> Self {
        Self {
            jsonrpc: default_jsonrpc_version(),
            id: Some(RequestId::Integer(0)), // default null ID
            result: Value::Null,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CallToolRequestParam {
    pub name: Cow<'static, str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
}

impl JSONRPCResponse {
    pub fn new(id: RequestId, result: Value) -> Self {
        Self {
            jsonrpc: default_jsonrpc_version(),
            id: Some(id.clone()),
            result,
        }
    }
    pub fn new_without_id(result: Value) -> Self {
        Self {
            jsonrpc: default_jsonrpc_version(),
            id: None,
            result,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JSONRPCError {
    pub jsonrpc: String,
    pub id: RequestId,
    pub error: JSONRPCErrorDetails,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JSONRPCErrorDetails {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

// Helper function to create JSON-RPC response
pub fn create_json_rpc_response(request_id: &str, body: &Option<Bytes>) -> Result<JSONRPCResponse> {
    let result = CallToolResult {
        meta: Map::new(),
        content: vec![CallToolResultContentItem::TextContent(TextContent {
            type_: "text".to_string(),
            text: body.as_ref().map_or_else(
                || ERROR_MESSAGE.to_string(),
                |b| String::from_utf8_lossy(b).to_string(),
            ),
            annotations: None,
        })],
        is_error: Some(false),
    };

    request_id
        .parse::<i64>()
        .map_err(|e| {
            log::error!("Invalid MCP-REQUEST-ID format: {}", e);
            Error::because(ErrorType::InvalidHTTPHeader, "Invalid MCP-REQUEST-ID", e)
        })
        .map(|id| JSONRPCResponse::new(RequestId::from(id), serde_json::to_value(result).unwrap()))
}
