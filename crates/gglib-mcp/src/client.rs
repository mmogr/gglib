//! MCP JSON-RPC client for communicating with MCP servers.
//!
//! Implements the MCP protocol over stdio (JSON-RPC 2.0).
//! Reference: <https://spec.modelcontextprotocol.io/>
//!
//! The server's stdout is read asynchronously, so waiting on a slow server
//! holds no runtime thread and the reply timeout fires. One request is in
//! flight at a time: a request holds the pipes from writing its line until
//! the reply whose `id` is its own arrives, and every other line before it
//! (a notification, a reply to an earlier request that timed out, startup
//! noise) is skipped.

use gglib_core::utils::process::async_cmd;
use gglib_core::{McpTool, McpToolResult};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::process::Child;
use tokio::sync::Mutex;
use tokio::time::timeout;

/// How long a request waits for its reply: long enough for a server started
/// through `npx` to come up.
const REPLY_TIMEOUT: Duration = Duration::from_secs(30);

/// Errors that can occur during MCP client operations.
#[derive(Debug, Error)]
pub(crate) enum McpClientError {
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
pub(crate) struct InitializeResult {
    #[serde(rename = "protocolVersion")]
    pub protocol_version: String,
    #[serde(rename = "serverInfo")]
    pub server_info: ServerInfo,
    pub capabilities: ServerCapabilities,
}

/// Server information from initialize.
///
/// Both fields are parsed and neither is read. They are kept because this
/// struct states the shape of the `serverInfo` object in the MCP handshake,
/// and a client that cannot name the server it connected to is a gap worth
/// leaving visible rather than deleting. The allows are per-field so that a
/// *new* unread field here still gets reported.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ServerInfo {
    #[allow(dead_code)]
    pub name: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub version: Option<String>,
}

/// Server capabilities.
///
/// Only `tools` is consulted (by `list_tools`). The other two are the rest of
/// the capability set the spec defines; same reasoning as [`ServerInfo`].
#[derive(Debug, Clone, Deserialize, Default)]
pub(crate) struct ServerCapabilities {
    #[serde(default)]
    pub tools: Option<ToolsCapability>,
    #[serde(default)]
    #[allow(dead_code)]
    pub resources: Option<Value>,
    #[serde(default)]
    #[allow(dead_code)]
    pub prompts: Option<Value>,
}

/// Tools capability.
///
/// `list_changed` advertises that the server will send `tools/list_changed`
/// notifications. Nothing here subscribes to those yet, so it is parsed and
/// ignored.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ToolsCapability {
    #[serde(default)]
    #[allow(dead_code)]
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
    /// Raw MCP annotations object (spec 2025-03-26).
    /// We extract `title` from this during conversion to `McpTool`.
    #[serde(default)]
    annotations: Option<Value>,
}

/// The server's stdin and stdout, held together by the one request in
/// flight.
struct Pipes {
    writer: Box<dyn AsyncWrite + Send + Unpin>,
    reader: BufReader<Box<dyn AsyncRead + Send + Unpin>>,
}

/// Client for communicating with an MCP server via stdio.
pub(crate) struct McpClient {
    /// Child process (for stdio servers)
    process: Option<Child>,
    /// The server's stdin and stdout, while connected
    pipes: Option<Mutex<Pipes>>,
    /// How long a request waits for its reply
    reply_timeout: Duration,
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
    pub(crate) const fn new() -> Self {
        Self {
            process: None,
            pipes: None,
            reply_timeout: REPLY_TIMEOUT,
            request_id: AtomicU64::new(1),
            server_info: None,
            capabilities: None,
            protocol_version: None,
        }
    }

    /// Connect to an MCP server by spawning a stdio process.
    pub(crate) async fn connect_stdio(
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

        let mut command = async_cmd(exe_path);
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
        self.pipes = Some(Mutex::new(Pipes {
            writer: Box::new(stdin),
            reader: BufReader::new(Box::new(stdout)),
        }));

        // Initialize the MCP session
        self.initialize().await
    }

    /// Send the initialize request to establish MCP session.
    async fn initialize(&mut self) -> Result<InitializeResult, McpClientError> {
        let params = json!({
            "protocolVersion": "2024-11-05",
            "clientInfo": {
                "name": "gglib",
                "version": gglib_build_info::SEMVER
            },
            "capabilities": {}
        });

        let result: InitializeResult = self.request("initialize", Some(params)).await?;

        self.server_info = Some(result.server_info.clone());
        self.capabilities = Some(result.capabilities.clone());
        self.protocol_version = Some(result.protocol_version.clone());

        // Send initialized notification
        self.notify("notifications/initialized", None).await?;

        Ok(result)
    }

    /// List available tools from the MCP server.
    pub(crate) async fn list_tools(&self) -> Result<Vec<McpTool>, McpClientError> {
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
            .map(|t| {
                let title = t
                    .annotations
                    .as_ref()
                    .and_then(|a| a.get("title"))
                    .and_then(|v| v.as_str())
                    .map(str::to_owned);
                McpTool {
                    name: t.name,
                    description: t.description,
                    input_schema: t.input_schema,
                    title,
                }
            })
            .collect())
    }

    /// Call a tool on the MCP server.
    pub(crate) async fn call_tool(
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

    /// Send a JSON-RPC request and wait for the reply with its `id`.
    ///
    /// Lines that are not that reply are skipped, however many come first:
    /// notifications and server requests (anything with a `method`), replies
    /// to other ids, and lines that are not JSON. An error reply with a null
    /// `id`, the server's answer to a line it could not read, is the reply.
    ///
    /// One request is in flight per server: callers queue on the pipes' lock,
    /// and the wait for the lock is not counted against the reply timeout,
    /// which bounds writing the request and reading its reply. A request
    /// cancelled part way through writing its line leaves the rest unwritten,
    /// and the server reads the next request run on from it; that request
    /// then fails or times out.
    async fn request<T: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> Result<T, McpClientError> {
        let pipes = self.pipes.as_ref().ok_or(McpClientError::NotConnected)?;
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);
        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params,
        };
        let request_line = serde_json::to_string(&request)? + "\n";

        let mut pipes = pipes.lock().await;
        let response = timeout(self.reply_timeout, async {
            pipes.writer.write_all(request_line.as_bytes()).await?;
            pipes.writer.flush().await?;
            read_reply(&mut pipes.reader, id).await
        })
        .await
        .map_err(|_| McpClientError::Timeout)??;
        drop(pipes);

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
    async fn notify(&self, method: &str, params: Option<Value>) -> Result<(), McpClientError> {
        let pipes = self.pipes.as_ref().ok_or(McpClientError::NotConnected)?;

        // Notifications don't have an id
        let notification = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params.unwrap_or_else(|| json!({}))
        });

        let line = serde_json::to_string(&notification)? + "\n";

        let mut pipes = pipes.lock().await;
        pipes.writer.write_all(line.as_bytes()).await?;
        pipes.writer.flush().await?;
        drop(pipes);

        Ok(())
    }

    /// Disconnect from the MCP server.
    pub(crate) fn disconnect(&mut self) {
        // Drop stdin to signal EOF to the child process.
        self.pipes = None;

        // Kill the process without waiting for it to exit: this runs in a
        // synchronous Drop, where waiting would block a runtime thread.
        // `start_kill` sends SIGKILL and returns; `try_wait` reaps a child
        // that has already exited, and tokio reaps one that has not once the
        // dropped `Child` exits.
        if let Some(mut process) = self.process.take() {
            let _ = process.start_kill();
            let _ = process.try_wait();
        }

        self.server_info = None;
        self.capabilities = None;
        self.protocol_version = None;
    }
}

/// Read lines until the reply to request `id`.
///
/// # Errors
///
/// [`McpClientError::ProtocolError`] when the server closes its stdout first,
/// [`McpClientError::IoError`] when reading fails.
async fn read_reply<R>(reader: &mut R, id: u64) -> Result<JsonRpcResponse, McpClientError>
where
    R: AsyncBufReadExt + Unpin,
{
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).await? == 0 {
            return Err(McpClientError::ProtocolError(
                "Server closed connection".to_string(),
            ));
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(message) = serde_json::from_str::<Value>(trimmed) else {
            // Not JSON: startup output from a launcher such as npx, or the
            // rest of a reply cut off by a timeout, which can be image
            // base64, so only its length is logged.
            tracing::debug!(bytes = trimmed.len(), "Skipping non-JSON-RPC output");
            continue;
        };
        if let Some(method) = message.get("method").and_then(Value::as_str) {
            tracing::debug!(
                method,
                "Skipping a message from the MCP server while awaiting a reply"
            );
            continue;
        }
        let unreadable = message.get("id") == Some(&Value::Null) && message.get("error").is_some();
        if !unreadable && message.get("id").and_then(Value::as_u64) != Some(id) {
            // Only the id: a late reply to a timed-out call can be megabytes of image.
            tracing::debug!(want = id, got = ?message.get("id"), "Skipping a reply to another request");
            continue;
        }
        return serde_json::from_value(message).map_err(McpClientError::from);
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
#[path = "client_tests.rs"]
mod client_tests;
