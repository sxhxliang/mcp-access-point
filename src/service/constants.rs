// Shared constants for service module

/// Context var key to mark StreamableHTTP handling mode
pub const MCP_STREAMABLE_HTTP: &str = "streamable_http";
/// HTTP header key for session id
pub const MCP_SESSION_ID: &str = "mcp-session-id";
/// Context var key for current JSON-RPC request id
pub const MCP_REQUEST_ID: &str = "mcp-request-id";
/// Var and header name for tenant id scoping
pub const MCP_TENANT_ID: &str = "MCP_TENANT_ID";
/// Var key for rewritten upstream request body
pub const NEW_BODY: &str = "new_body";
/// Var key for rewritten upstream request body length
pub const NEW_BODY_LEN: &str = "new_body_len";
