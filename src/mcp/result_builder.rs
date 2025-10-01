use serde_json::Map;

use crate::types::{CallToolResult, CallToolResultContentItem, TextContent};

/// Builder for creating standardized CallToolResult responses
pub struct ResultBuilder;

impl ResultBuilder {
    /// Creates an error result with the given message
    pub fn error(message: &str) -> CallToolResult {
        CallToolResult {
            meta: Map::new(),
            content: vec![CallToolResultContentItem::TextContent(TextContent {
                type_: "text".to_string(),
                text: message.to_string(),
                annotations: None,
            })],
            is_error: Some(true),
        }
    }

    /// Creates a not found error result for a specific tool
    pub fn tool_not_found(tool_name: &str) -> CallToolResult {
        CallToolResult {
            meta: Map::new(),
            content: vec![CallToolResultContentItem::TextContent(TextContent {
                type_: "text".to_string(),
                text: format!("Tool not found: {}", tool_name),
                annotations: None,
            })],
            is_error: Some(false),
        }
    }

    /// Creates a missing parameters error result
    pub fn missing_params() -> CallToolResult {
        Self::error("Missing request parameters")
    }

    /// Creates an invalid parameters error result
    pub fn invalid_params() -> CallToolResult {
        Self::error("Invalid request parameters")
    }
}