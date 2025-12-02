//! MCP server configuration types.
//!
//! These types are shared between Rust backend and TypeScript frontend.

use serde::{Deserialize, Serialize};

/// Type of MCP server connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpServerType {
    /// Stdio-based server - gglib spawns and manages the process
    Stdio,
    /// SSE-based server - external process, gglib connects via HTTP
    Sse,
}

impl Default for McpServerType {
    fn default() -> Self {
        Self::Stdio
    }
}

/// Runtime status of an MCP server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpServerStatus {
    /// Server is not running
    Stopped,
    /// Server is starting up
    Starting,
    /// Server is running and connected
    Running,
    /// Server encountered an error
    Error(String),
}

impl Default for McpServerStatus {
    fn default() -> Self {
        Self::Stopped
    }
}

/// Configuration for an MCP server.
///
/// This struct is persisted to the database and shared with the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Unique identifier (set by database on insert)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,

    /// User-friendly name for the server
    pub name: String,

    /// Connection type (stdio or sse)
    #[serde(rename = "type")]
    pub server_type: McpServerType,

    /// Whether tools from this server are included in chat
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Whether to start this server when gglib launches
    #[serde(default)]
    pub auto_start: bool,

    // --- Stdio server fields ---
    /// Command to run (e.g., "npx", "python", "node")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,

    /// Arguments to pass to the command
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,

    /// Working directory for the process
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,

    // --- SSE server fields ---
    /// URL for SSE connection (e.g., "http://localhost:3001/sse")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    // --- Environment variables (sensitive data) ---
    /// Environment variables as key-value pairs
    /// Note: Values are stored encrypted in the database
    #[serde(default)]
    pub env: Vec<(String, String)>,

    // --- Metadata ---
    /// When the server was added
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,

    /// Last successful connection time
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_connected_at: Option<String>,
}

fn default_true() -> bool {
    true
}

impl McpServerConfig {
    /// Create a new stdio-based MCP server config.
    pub fn new_stdio(
        name: impl Into<String>,
        command: impl Into<String>,
        args: Vec<String>,
    ) -> Self {
        Self {
            id: None,
            name: name.into(),
            server_type: McpServerType::Stdio,
            enabled: true,
            auto_start: false,
            command: Some(command.into()),
            args: Some(args),
            cwd: None,
            url: None,
            env: Vec::new(),
            created_at: None,
            last_connected_at: None,
        }
    }

    /// Create a new SSE-based MCP server config.
    pub fn new_sse(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            id: None,
            name: name.into(),
            server_type: McpServerType::Sse,
            enabled: true,
            auto_start: false,
            command: None,
            args: None,
            cwd: None,
            url: Some(url.into()),
            env: Vec::new(),
            created_at: None,
            last_connected_at: None,
        }
    }

    /// Add an environment variable.
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    /// Set the working directory.
    pub fn with_cwd(mut self, cwd: impl Into<String>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// Set auto-start.
    pub fn with_auto_start(mut self, auto_start: bool) -> Self {
        self.auto_start = auto_start;
        self
    }
}

/// Tool definition from an MCP server.
///
/// This matches the OpenAI tool format used by the frontend Tool Registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    /// Tool name (function name)
    pub name: String,

    /// Human-readable description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// JSON Schema for input parameters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,
}

/// Result of a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolResult {
    /// Whether the call succeeded
    pub success: bool,

    /// Result data (if success)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,

    /// Error message (if failed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl McpToolResult {
    /// Create a success result.
    pub fn success(data: serde_json::Value) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    /// Create an error result.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message.into()),
        }
    }
}

/// Server status with additional runtime information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    /// Server configuration
    pub config: McpServerConfig,

    /// Current runtime status
    pub status: McpServerStatus,

    /// Tools exposed by this server (populated when running)
    #[serde(default)]
    pub tools: Vec<McpTool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stdio_config() {
        let config = McpServerConfig::new_stdio(
            "Test Server",
            "npx",
            vec!["-y".to_string(), "@test/mcp-server".to_string()],
        )
        .with_env("API_KEY", "secret123")
        .with_auto_start(true);

        assert_eq!(config.name, "Test Server");
        assert_eq!(config.server_type, McpServerType::Stdio);
        assert_eq!(config.command, Some("npx".to_string()));
        assert_eq!(config.env.len(), 1);
        assert_eq!(
            config.env[0],
            ("API_KEY".to_string(), "secret123".to_string())
        );
        assert!(config.auto_start);
    }

    #[test]
    fn test_sse_config() {
        let config = McpServerConfig::new_sse("External Server", "http://localhost:3001/sse");

        assert_eq!(config.name, "External Server");
        assert_eq!(config.server_type, McpServerType::Sse);
        assert_eq!(config.url, Some("http://localhost:3001/sse".to_string()));
        assert!(config.command.is_none());
    }

    #[test]
    fn test_serialization() {
        let config = McpServerConfig::new_stdio("Test", "node", vec!["server.js".to_string()]);
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"type\":\"stdio\""));
        assert!(json.contains("\"name\":\"Test\""));
    }
}
