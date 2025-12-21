//! MCP JSON-RPC client for communicating with MCP servers.
//!
//! Implements the MCP protocol over stdio (JSON-RPC 2.0).
//! Reference: <https://spec.modelcontextprotocol.io/>
#![allow(dead_code)] // Some protocol fields/methods not yet used by callers

use gglib_core::{McpTool, McpToolResult};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::time::timeout;

/// Errors that can occur during MCP client operations.
#[derive(Debug, Error)]
pub enum McpClientError {
    #[error("Failed to spawn MCP server process: {0}")]
    SpawnFailed(String),

    #[error("Failed to communicate with MCP server: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("MCP protocol error: {0}")]
    ProtocolError(String),

    #[error("Timeout waiting for MCP server response")]
    Timeout,

    #[error("MCP server returned error: code={code}, message={message}")]
    ServerError { code: i64, message: String },

    #[error("Server not connected")]
    NotConnected,
}

/// JSON-RPC 2.0 request.
#[derive(Debug, Serialize)]
struct JsonRpcRequest {
    jsonrpc: &'static str,
    id: u64,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

/// JSON-RPC 2.0 response.
#[derive(Debug, Deserialize)]
#[allow(dead_code)] // Fields required by serde deserialization, verified in tests
struct JsonRpcResponse {
    jsonrpc: String,
    id: Option<u64>,
    result: Option<Value>,
    error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 error.
#[derive(Debug, Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
    #[serde(rename = "data")]
    _data: Option<Value>,
}

/// MCP initialize result.
#[derive(Debug, Clone, Deserialize)]
pub struct InitializeResult {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    #[serde(rename = "serverInfo")]
    pub server_info: ServerInfo,
    pub capabilities: ServerCapabilities,
}

/// Server information from initialize.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
}

/// Server capabilities.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ServerCapabilities {
    #[serde(default)]
    pub tools: Option<ToolsCapability>,
    #[serde(default)]
    pub resources: Option<Value>,
    #[serde(default)]
    pub prompts: Option<Value>,
}

/// Tools capability.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolsCapability {
    #[serde(default)]
    pub list_changed: Option<bool>,
}

/// MCP tool from tools/list.
#[derive(Debug, Deserialize)]
struct McpToolSchema {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default, rename = "inputSchema")]
    input_schema: Option<Value>,
}

/// Client for communicating with an MCP server via stdio.
pub struct McpClient {
    /// Child process (for stdio servers)
    process: Option<Child>,
    /// Stdin for sending requests (wrapped for async access)
    stdin: Option<Arc<std::sync::Mutex<ChildStdin>>>,
    /// Stdout reader for receiving responses
    stdout_reader: Option<Arc<Mutex<BufReader<ChildStdout>>>>,
    /// Request ID counter
    request_id: AtomicU64,
    /// Server info after initialization
    server_info: Option<ServerInfo>,
    /// Server capabilities
    capabilities: Option<ServerCapabilities>,
    /// Protocol version
    protocol_version: Option<String>,
}

impl McpClient {
    /// Create a new MCP client (not yet connected).
    pub const fn new() -> Self {
        Self {
            process: None,
            stdin: None,
            stdout_reader: None,
            request_id: AtomicU64::new(1),
            server_info: None,
            capabilities: None,
            protocol_version: None,
        }
    }

    /// Connect to an MCP server by spawning a stdio process.
    pub async fn connect_stdio(
        &mut self,
        exe_path: &str,
        args: &[String],
        cwd: Option<&str>,
        path_extra: Option<&str>,
        env: &[(String, String)],
    ) -> Result<InitializeResult, McpClientError> {
        // Validate executable path before attempting spawn
        crate::path::validate_exe_path(exe_path).map_err(McpClientError::SpawnFailed)?;

        // Validate working directory if specified
        if let Some(working_dir) = cwd {
            crate::path::validate_working_dir(working_dir).map_err(McpClientError::SpawnFailed)?;
        }

        // Build effective PATH for child process
        let effective_path = crate::path::build_effective_path(exe_path, path_extra);

        let mut command = std::process::Command::new(exe_path);
        command
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("PATH", &effective_path); // Set enriched PATH for child

        if let Some(working_dir) = cwd {
            command.current_dir(working_dir);
        }

        // Add user-provided environment variables (after PATH)
        for (key, value) in env {
            command.env(key, value);
        }

        let mut child = command.spawn().map_err(|e| {
            // Build detailed error message with all context
            let effective_path_str = effective_path.to_string_lossy();
            McpClientError::SpawnFailed(format!(
                "Failed to spawn '{exe_path}': {e}\nArgs: {args:?}\nCwd: {cwd:?}\nEffective PATH: {effective_path_str}"
            ))
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| McpClientError::SpawnFailed("Failed to get stdin".to_string()))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| McpClientError::SpawnFailed("Failed to get stdout".to_string()))?;

        self.process = Some(child);
        self.stdin = Some(Arc::new(std::sync::Mutex::new(stdin)));
        self.stdout_reader = Some(Arc::new(Mutex::new(BufReader::new(stdout))));

        // Initialize the MCP session
        self.initialize().await
    }

    /// Send the initialize request to establish MCP session.
    async fn initialize(&mut self) -> Result<InitializeResult, McpClientError> {
        let params = json!({
            "protocolVersion": "2024-11-05",
            "clientInfo": {
                "name": "gglib",
                "version": env!("CARGO_PKG_VERSION")
            },
            "capabilities": {}
        });

        let result: InitializeResult = self.request("initialize", Some(params)).await?;

        self.server_info = Some(result.server_info.clone());
        self.capabilities = Some(result.capabilities.clone());
        self.protocol_version = Some(result.protocol_version.clone());

        // Send initialized notification
        self.notify("notifications/initialized", None)?;

        Ok(result)
    }

    /// List available tools from the MCP server.
    pub async fn list_tools(&self) -> Result<Vec<McpTool>, McpClientError> {
        // Check if server supports tools
        if self
            .capabilities
            .as_ref()
            .and_then(|c| c.tools.as_ref())
            .is_none()
        {
            return Ok(Vec::new());
        }

        let result: Value = self.request("tools/list", None).await?;

        let tools_value = result.get("tools").cloned().unwrap_or(json!([]));
        let mcp_tools: Vec<McpToolSchema> = serde_json::from_value(tools_value)?;

        Ok(mcp_tools
            .into_iter()
            .map(|t| McpTool {
                name: t.name,
                description: t.description,
                input_schema: t.input_schema,
            })
            .collect())
    }

    /// Call a tool on the MCP server.
    pub async fn call_tool(
        &self,
        name: &str,
        arguments: HashMap<String, Value>,
    ) -> Result<McpToolResult, McpClientError> {
        let params = json!({
            "name": name,
            "arguments": arguments
        });

        let result: Value = self.request("tools/call", Some(params)).await?;

        // MCP returns content array with text/image items
        let content = result.get("content").cloned().unwrap_or(json!([]));
        let is_error = result
            .get("isError")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        if is_error {
            // Extract error message from content
            let error_msg = content
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|item| item.get("text"))
                .and_then(|t| t.as_str())
                .unwrap_or("Unknown error")
                .to_string();

            Ok(McpToolResult::error(error_msg))
        } else {
            // Return content as the result
            Ok(McpToolResult::success(content))
        }
    }

    /// Send a JSON-RPC request and wait for response.
    async fn request<T: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> Result<T, McpClientError> {
        let stdin = self.stdin.as_ref().ok_or(McpClientError::NotConnected)?;
        let stdout_reader = self
            .stdout_reader
            .as_ref()
            .ok_or(McpClientError::NotConnected)?;

        let id = self.request_id.fetch_add(1, Ordering::SeqCst);

        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params,
        };

        // Write request
        let request_line = serde_json::to_string(&request)? + "\n";

        // Use blocking IO wrapped in std Mutex
        {
            let mut stdin_guard = stdin
                .lock()
                .map_err(|_| McpClientError::ProtocolError("Failed to lock stdin".to_string()))?;
            stdin_guard.write_all(request_line.as_bytes())?;
            stdin_guard.flush()?;
        }

        // Read response with timeout (30 seconds for initial startup, especially for npx)
        let read_timeout = Duration::from_secs(30);

        let read_result = timeout(read_timeout, async {
            let mut reader = stdout_reader.lock().await;

            // Try reading lines until we get a valid JSON-RPC response
            // (skip any empty lines or non-JSON output from npx startup)
            for _ in 0..10 {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => {
                        // EOF - server closed stdout
                        return Err(McpClientError::ProtocolError(
                            "Server closed connection".to_string(),
                        ));
                    }
                    Ok(_) => {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            // Empty line, try again
                            tokio::time::sleep(Duration::from_millis(100)).await;
                            continue;
                        }

                        // Try to parse as JSON
                        if let Ok(response) = serde_json::from_str::<JsonRpcResponse>(trimmed) {
                            return Ok(response);
                        }
                        // Not valid JSON-RPC, might be npx output, skip it
                        tracing::debug!(line = trimmed, "Skipping non-JSON-RPC output");
                    }
                    Err(e) => return Err(McpClientError::IoError(e)),
                }
            }

            Err(McpClientError::ProtocolError(
                "No valid JSON-RPC response received".to_string(),
            ))
        })
        .await;

        let response = match read_result {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err(McpClientError::Timeout),
        };

        // Check for error
        if let Some(err) = response.error {
            return Err(McpClientError::ServerError {
                code: err.code,
                message: err.message,
            });
        }

        // Parse result
        let result = response.result.ok_or_else(|| {
            McpClientError::ProtocolError("Missing result in response".to_string())
        })?;

        serde_json::from_value(result).map_err(std::convert::Into::into)
    }

    /// Send a JSON-RPC notification (no response expected).
    fn notify(&self, method: &str, params: Option<Value>) -> Result<(), McpClientError> {
        let stdin = self.stdin.as_ref().ok_or(McpClientError::NotConnected)?;

        // Notifications don't have an id
        let notification = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params.unwrap_or_else(|| json!({}))
        });

        let line = serde_json::to_string(&notification)? + "\n";

        {
            let mut stdin_guard = stdin
                .lock()
                .map_err(|_| McpClientError::ProtocolError("Failed to lock stdin".to_string()))?;
            stdin_guard.write_all(line.as_bytes())?;
            stdin_guard.flush()?;
        }

        Ok(())
    }

    /// Check if the client is connected.
    pub const fn is_connected(&self) -> bool {
        self.stdin.is_some() && self.process.is_some()
    }

    /// Get server info (available after initialize).
    pub const fn server_info(&self) -> Option<&ServerInfo> {
        self.server_info.as_ref()
    }

    /// Disconnect from the MCP server.
    pub fn disconnect(&mut self) {
        // Drop stdin to signal EOF
        self.stdin = None;
        self.stdout_reader = None;

        // Kill the process if still running
        if let Some(mut process) = self.process.take() {
            let _ = process.kill();
            let _ = process.wait();
        }

        self.server_info = None;
        self.capabilities = None;
        self.protocol_version = None;
    }
}

impl Default for McpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        self.disconnect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_rpc_request_serialization() {
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id: 1,
            method: "tools/list".to_string(),
            params: None,
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"method\":\"tools/list\""));
        assert!(!json.contains("params")); // Should be omitted when None
    }

    #[test]
    fn test_json_rpc_response_parsing() {
        let json = r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#;
        let response: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, Some(1));
        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }

    #[test]
    fn test_json_rpc_error_parsing() {
        let json =
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32600,"message":"Invalid Request"}}"#;
        let response: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert!(response.error.is_some());
        assert_eq!(response.error.as_ref().unwrap().code, -32600);
    }
}
