use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{borrow::Cow, collections::HashMap};

pub const LATEST_PROTOCOL_VERSION: &str = "2024-11-05";
pub const JSONRPC_VERSION: &str = "2.0";

pub type Cursor = String;
pub type RequestId = i32;

pub const PARSE_ERROR: i32 = -32700;
pub const INVALID_REQUEST: i32 = -32600;
pub const METHOD_NOT_FOUND: i32 = -32601;
pub const INVALID_PARAMS: i32 = -32602;
pub const INTERNAL_ERROR: i32 = -32603;

enum ErrorCode {
    // Standard JSON-RPC error codes
    ParseError = -32700,
    InvalidRequest = -32600,
    MethodNotFound = -32601,
    InvalidParams = -32602,
    InternalError = -32603,
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

    pub id: RequestId,

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
            id: 0, // default null ID
            result: Value::Null,
        }
    }
}

impl JSONRPCResponse {
    pub fn new(id: RequestId, result: Value) -> Self {
        Self {
            jsonrpc: default_jsonrpc_version(),
            id,
            result,
        }
    }
    pub fn new_default() -> Self {
        Self::default()
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
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeRequest {
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<InitializeParams>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    pub protocol_version: String,
    pub capabilities: ClientCapabilities,
    pub client_info: Implementation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    pub server_info: Implementation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental: Option<HashMap<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roots: Option<RootsCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental: Option<HashMap<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logging: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<PromptsCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<ResourcesCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RootsCapability {
    pub list_changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptsCapability {
    pub list_changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcesCapability {
    pub subscribe: bool,
    pub list_changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsCapability {
    pub list_changed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Implementation {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    pub uri: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListResourcesResult {
    pub resources: Vec<Resource>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReadResourceRequestParam {
    pub uri: String,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReadResourceResult {
    pub contents: Vec<ResourceContents>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceTemplate {
    pub uri_template: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ResourceTemplateListResult {
    pub resource_templates: Vec<ResourceTemplate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResourceContents {
    Text(TextResourceContents),
    Blob(BlobResourceContents),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextResourceContents {
    pub uri: String,
    pub mime_type: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Prompt {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<PromptArgument>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListPromptsResult {
    pub prompts: Vec<Prompt>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptArgument {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PromptMessageContent {
    Text(TextContent),
    Image(ImageContent),
    Resource(EmbeddedResource),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptMessage {
    pub role: Role,
    pub content: PromptMessageContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListToolsResult {
    pub tools: Vec<Tool>,
}
impl ListToolsResult {
    pub fn new(tools: Vec<Tool>) -> Self {
        Self { tools }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_schema: ToolInputSchema,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInputSchema {
    #[serde(rename = "type")]
    pub schema_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<HashMap<String, ToolInputSchemaProperty>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInputSchemaProperty {
    pub title: String,
    #[serde(rename = "type")]
    pub prop_type: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CallToolRequestParam {
    pub name: Cow<'static, str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CallToolResult {
    pub content: Vec<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Content {
    Text(TextContent),
    Image(ImageContent),
    Resource(EmbeddedResource),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoggingLevel {
    Debug,
    Info,
    Notice,
    Warning,
    Error,
    Critical,
    Alert,
    Emergency,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoggingMessageNotification {
    pub method: String,
    pub params: LoggingMessageParams,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoggingMessageParams {
    pub level: LoggingLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logger: Option<String>,
    pub data: Value,
}

// 采样相关
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SamplingMessage {
    pub role: Role,
    pub content: SamplingContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SamplingContent {
    Text(TextContent),
    Image(ImageContent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextContent {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageContent {
    pub data: String,
    pub mime_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Annotated {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Annotations {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audience: Option<Vec<Role>>,

    ///  (0.0-1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlobResourceContents {
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(rename = "blob")]
    pub base64_data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedResource {
    #[serde(flatten)]
    pub base: Annotated,

    #[serde(rename = "type")]
    pub resource_type: String, // "resource"

    pub resource: ResourceContents,
}

impl Default for EmbeddedResource {
    fn default() -> Self {
        Self {
            base: Annotated { annotations: None },
            resource_type: "resource".to_string(),
            resource: ResourceContents::Text(TextResourceContents {
                uri: String::new(),
                mime_type: None,
                text: String::new(),
            }),
        }
    }
}

impl EmbeddedResource {
    pub fn new_text(uri: String, text: String, mime_type: Option<String>) -> Self {
        Self {
            resource_type: "resource".to_string(),
            resource: ResourceContents::Text(TextResourceContents {
                uri,
                mime_type,
                text,
            }),
            ..Default::default()
        }
    }

    pub fn new_blob(uri: String, base64_data: String, mime_type: Option<String>) -> Self {
        Self {
            resource_type: "resource".to_string(),
            resource: ResourceContents::Blob(BlobResourceContents {
                uri,
                mime_type,
                base64_data,
            }),
            ..Default::default()
        }
    }

    /// 添加注解
    pub fn with_annotations(mut self, annotations: Annotations) -> Self {
        self.base.annotations = Some(annotations);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaginatedResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<Cursor>,
    #[serde(flatten)]
    pub additional: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ClientRequest {
    Ping(PingRequest),
    Initialize(InitializeRequest),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingRequest {
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SamplingRequest {
    pub messages: Vec<Message>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_preferences: Option<ModelPreferences>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_context: Option<IncludeContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    pub max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub role: Role,
    pub content: MessageContent,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageContent {
    #[serde(rename = "type")]
    pub content_type: MessageContentType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageContentType {
    Text,
    Image,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPreferences {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hints: Option<Vec<ModelHint>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_priority: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed_priority: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intelligence_priority: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelHint {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IncludeContext {
    None,
    ThisServer,
    AllServers,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SamplingResponse {
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<StopReason>,
    pub role: Role,
    pub content: MessageContent,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StopReason {
    EndTurn,
    StopSequence,
    MaxTokens,
    #[serde(untagged)]
    Other(String),
}

#[test]
fn initialize_result_serialize_instructions_none() {
    let result = InitializeResult {
        protocol_version: "2024-11-05".to_string(),
        capabilities: ServerCapabilities {
            experimental: Some(HashMap::new()),
            logging: None,
            prompts: Some(PromptsCapability {
                list_changed: false,
            }),
            resources: Some(ResourcesCapability {
                subscribe: false,
                list_changed: false,
            }),
            tools: Some(ToolsCapability {
                list_changed: false,
            }),
        },
        server_info: Implementation {
            name: "weather".to_string(),
            version: "1.5.0".to_string(),
        },
        instructions: None,
    };

    let expected_json = serde_json::json!({
        "protocolVersion": "2024-11-05",
        "capabilities": {
            "experimental": {},
            "prompts": {
                "listChanged": false
            },
            "resources": {
                "subscribe": false,
                "listChanged": false
            },
            "tools": {
                "listChanged": false
            }
        },
        "serverInfo": {
            "name": "weather",
            "version": "1.5.0"
        }
    });

    assert_eq!(serde_json::to_value(result).unwrap(), expected_json);
}

#[test]
fn initialize_result_serialize_all_fields_present() {
    let expected_json = serde_json::json!({
        "resourceTemplates": [
            {
                "uriTemplate": "greeting://{name}",
                "name": "get_greeting",
                "description": "Get a personalized greeting",
                "mimeType": "image/jpeg"
            },
            {
                "uriTemplate": "users://{user_id}/profile",
                "name": "get_user_profile",
                "description": "Dynamic user data"
            }
        ]
    });
    let result = ResourceTemplateListResult {
        resource_templates: vec![
            ResourceTemplate {
                uri_template: "greeting://{name}".to_string(),
                name: "get_greeting".to_string(),
                description: Some("Get a personalized greeting".to_string()),
                mime_type: Some("image/jpeg".to_string()),
            },
            ResourceTemplate {
                uri_template: "users://{user_id}/profile".to_string(),
                name: "get_user_profile".to_string(),
                description: Some("Dynamic user data".to_string()),
                mime_type: None,
            },
        ],
    };

    assert_eq!(serde_json::to_value(result).unwrap(), expected_json);
}

#[test]
fn prompts_list_test() {
    let expected_json = serde_json::json!({
        "prompts": [
            {
              "name": "current-time",
              "description": "Display current time in the city",
              "arguments": [
                {
                  "name": "city",
                  "description": "City name",
                  "required": true
                }
              ]
            },
            {
              "name": "analyze-code",
              "description": "Analyze code for potential improvements",
              "arguments": [
                {
                  "name": "language",
                  "description": "Programming language",
                  "required": true
                }
              ]
            }
        ]
    });
    let result = ListPromptsResult {
        prompts: vec![
            Prompt {
                name: "current-time".to_string(),
                description: Some("Display current time in the city".to_string()),
                arguments: Some(vec![PromptArgument {
                    name: "city".to_string(),
                    description: Some("City name".to_string()),
                    required: Some(true),
                }]),
            },
            Prompt {
                name: "analyze-code".to_string(),
                description: Some("Analyze code for potential improvements".to_string()),
                arguments: Some(vec![PromptArgument {
                    name: "language".to_string(),
                    description: Some("Programming language".to_string()),
                    required: Some(true),
                }]),
            },
        ],
    };
    assert_eq!(serde_json::to_value(result).unwrap(), expected_json);
}
