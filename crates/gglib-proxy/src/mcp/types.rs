//! JSON-RPC 2.0 and MCP Streamable HTTP protocol types.
//!
//! Defines the wire types for the MCP Streamable HTTP transport
//! (spec version 2025-03-26). The proxy receives JSON-RPC requests
//! at `POST /mcp` and returns either `application/json` or
//! `text/event-stream` responses.
//!
//! These types are independent of `gglib-mcp`'s internal client-side
//! JSON-RPC types (which are private to that crate). The gateway maps
//! between these wire types and `McpService` domain types.

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ─── JSON-RPC 2.0 error codes ──────────────────────────────────────────────

/// Parse error: invalid JSON was received.
pub(crate) const PARSE_ERROR: i32 = -32700;

/// Invalid request: the JSON sent is not a valid request object.
pub(crate) const INVALID_REQUEST: i32 = -32600;

/// Method not found: the method does not exist or is not available.
pub(crate) const METHOD_NOT_FOUND: i32 = -32601;

/// Invalid params: invalid method parameter(s).
pub(crate) const INVALID_PARAMS: i32 = -32602;

// ─── JSON-RPC 2.0 request / response ───────────────────────────────────────

/// A JSON-RPC 2.0 request or notification.
///
/// When `id` is `None`, this is a notification (no response expected).
/// When `id` is `Some`, the server must return a response with the same id.
#[derive(Debug, Deserialize)]
pub(crate) struct JsonRpcRequest {
    /// Must be `"2.0"`, and now checked to be: serde rejects a body that omits
    /// this field, and `post_mcp` rejects one that carries any other version.
    pub jsonrpc: String,
    /// Request identifier. Absent for notifications.
    pub id: Option<Value>,
    /// Method name (e.g. "initialize", "tools/list", "tools/call").
    pub method: String,
    /// Method parameters.
    pub params: Option<Value>,
}

/// A JSON-RPC 2.0 success or error response.
#[derive(Debug, Serialize)]
pub(crate) struct JsonRpcResponse {
    /// Always "2.0".
    pub jsonrpc: &'static str,
    /// Echoed from the request.
    pub id: Value,
    /// Present on success.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Present on error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    /// Build a success response.
    pub(crate) fn success(id: Value, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Build an error response.
    pub(crate) fn error(id: Value, error: JsonRpcError) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(error),
        }
    }
}

/// A JSON-RPC 2.0 error object.
#[derive(Debug, Serialize)]
pub(crate) struct JsonRpcError {
    /// Numeric error code.
    pub code: i32,
    /// Short description of the error.
    pub message: String,
    /// Optional structured error data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl JsonRpcError {
    /// Create a new error with just code and message.
    pub(crate) fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }
}

// ─── MCP protocol types (2025-03-26) ───────────────────────────────────────

/// Result of the `initialize` method.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InitializeResult {
    /// Protocol version the server supports.
    pub protocol_version: &'static str,
    /// Server capabilities.
    pub capabilities: ServerCapabilities,
    /// Server identification.
    pub server_info: ServerInfo,
}

/// Server identification sent during initialization.
#[derive(Debug, Serialize)]
pub(crate) struct ServerInfo {
    /// Server name.
    pub name: &'static str,
    /// Server version.
    pub version: &'static str,
}

/// Server capabilities advertised during initialization.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ServerCapabilities {
    /// Tool-related capabilities.
    pub tools: Option<ToolCapabilities>,
}

/// Tool capabilities.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolCapabilities {
    /// Whether the server will send `notifications/tools/list_changed`.
    pub list_changed: bool,
}

/// A single tool in `tools/list` response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct McpToolSpec {
    /// Qualified tool name (e.g. "server_name__tool_name").
    pub name: String,
    /// Human-readable description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// JSON Schema describing the tool's input parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<Value>,
    /// Human-readable display title from MCP `annotations.title`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// Result of the `tools/list` method.
#[derive(Debug, Serialize)]
pub(crate) struct ToolsListResult {
    /// Available tools.
    pub tools: Vec<McpToolSpec>,
}

/// A single content item in a `tools/call` response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ToolContent {
    /// Content type (usually "text").
    #[serde(rename = "type")]
    pub content_type: String,
    /// The content payload.
    pub text: String,
}

/// Result of the `tools/call` method.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CallToolResult {
    /// Content items returned by the tool.
    pub content: Vec<ToolContent>,
    /// Whether the tool call resulted in an error.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_initialize_request() {
        let json = r#"{
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": { "name": "OpenWebUI", "version": "0.6.31" }
            }
        }"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "initialize");
        assert!(req.id.is_some());
    }

    #[test]
    fn serialize_success_response() {
        let resp =
            JsonRpcResponse::success(Value::Number(1.into()), serde_json::json!({"status": "ok"}));
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"result\""));
        assert!(!json.contains("\"error\""));
    }

    #[test]
    fn serialize_error_response() {
        let resp = JsonRpcResponse::error(
            Value::Number(1.into()),
            JsonRpcError::new(METHOD_NOT_FOUND, "Method not found"),
        );
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"error\""));
        assert!(!json.contains("\"result\""));
        assert!(json.contains("-32601"));
    }

    #[test]
    fn deserialize_notification() {
        let json = r#"{
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert!(req.id.is_none());
        assert_eq!(req.method, "notifications/initialized");
    }
}
