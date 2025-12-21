//! MCP server domain types.
//!
//! These types are shared between Rust backend and TypeScript frontend.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Type of MCP server connection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpServerType {
    /// Stdio-based server - gglib spawns and manages the process
    #[default]
    Stdio,
    /// SSE-based server - external process, gglib connects via HTTP
    Sse,
}

/// Runtime status of an MCP server.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum McpServerStatus {
    /// Server is not running
    #[default]
    Stopped,
    /// Server is starting up
    Starting,
    /// Server is running and connected
    Running,
    /// Server encountered an error
    Error(String),
}

/// Environment variable entry for MCP servers.
///
/// Note: Values are stored as base64-encoded strings in the database.
/// This is encoding, NOT encryption. A follow-up task should add
/// proper at-rest protection (e.g., OS keychain integration).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpEnvEntry {
    /// Environment variable key
    pub key: String,
    /// Environment variable value (stored encoded, not encrypted)
    pub value: String,
}

impl McpEnvEntry {
    /// Create a new environment variable entry.
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

/// Execution configuration for an MCP server.
///
/// This contains the runtime configuration needed to start/connect to a server.
/// For stdio servers, `command` is required. For SSE servers, `url` is required.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpServerConfig {
    // --- Stdio server fields ---
    /// Command to execute (e.g., "npx" or "/opt/homebrew/bin/npx").
    /// Can be a simple command name (resolved via PATH + platform defaults)
    /// or an absolute path. Required for stdio servers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,

    /// Cached resolved absolute path to the executable.
    /// Populated by the path resolver when command is successfully resolved.
    /// Used to avoid repeated PATH searches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_path_cache: Option<String>,

    /// Arguments to pass to the executable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,

    /// Working directory for the process (must exist if specified)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,

    /// Additional PATH entries to prepend to the child process PATH.
    /// Useful for nvm/asdf shims or custom tool locations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_extra: Option<String>,

    // --- SSE server fields ---
    /// URL for SSE connection (e.g., `http://localhost:3001/sse`)
    /// Required for SSE servers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

impl McpServerConfig {
    /// Create a stdio server configuration.
    #[must_use]
    pub fn stdio(
        command: impl Into<String>,
        args: Vec<String>,
        working_dir: Option<String>,
        path_extra: Option<String>,
    ) -> Self {
        Self {
            command: Some(command.into()),
            resolved_path_cache: None,
            args: Some(args),
            working_dir,
            path_extra,
            url: None,
        }
    }

    /// Create an SSE server configuration.
    #[must_use]
    pub fn sse(url: impl Into<String>) -> Self {
        Self {
            command: None,
            resolved_path_cache: None,
            args: None,
            working_dir: None,
            path_extra: None,
            url: Some(url.into()),
        }
    }

    /// Validate configuration based on server type.
    ///
    /// Returns an error if required fields are missing or invalid for the server type.
    pub fn validate(&self, server_type: McpServerType) -> Result<(), String> {
        match server_type {
            McpServerType::Stdio => {
                // Stdio servers MUST have command
                let command = self
                    .command
                    .as_ref()
                    .ok_or_else(|| "Stdio server requires command".to_string())?;

                if command.is_empty() {
                    return Err("Stdio server command cannot be empty".to_string());
                }

                // working_dir MUST be absolute if specified
                if let Some(ref cwd) = self.working_dir {
                    if !cwd.is_empty() && !std::path::Path::new(cwd).is_absolute() {
                        return Err(format!("Stdio server working_dir must be absolute: {cwd}"));
                    }
                }

                Ok(())
            }
            McpServerType::Sse => {
                // SSE servers MUST have url
                let url = self
                    .url
                    .as_ref()
                    .ok_or_else(|| "SSE server requires url".to_string())?;

                if url.is_empty() {
                    return Err("SSE server url cannot be empty".to_string());
                }

                Ok(())
            }
        }
    }
}

/// An MCP server that exists in the system with a database ID.
///
/// This represents a persisted MCP server with all its metadata.
/// Use `NewMcpServer` for servers that haven't been persisted yet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    /// Database ID of the server (always present for persisted servers).
    pub id: i64,

    /// User-friendly name for the server.
    pub name: String,

    /// Connection type (stdio or SSE).
    pub server_type: McpServerType,

    /// Execution configuration (command, args, URL, etc.).
    pub config: McpServerConfig,

    /// Whether tools from this server are included in chat.
    pub enabled: bool,

    /// Whether to start this server when gglib launches.
    pub auto_start: bool,

    /// Environment variables for the server process.
    pub env: Vec<McpEnvEntry>,

    /// When the server was added.
    pub created_at: DateTime<Utc>,

    /// Last successful connection time.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_connected_at: Option<DateTime<Utc>>,

    /// Whether the server configuration is valid (`exe_path` exists and is executable).
    /// Updated during startup validation.
    pub is_valid: bool,

    /// Last validation or runtime error message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

/// An MCP server to be inserted into the system (no ID yet).
///
/// This represents an MCP server that hasn't been persisted to the database.
/// After insertion, the repository returns an `McpServer` with the assigned ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewMcpServer {
    /// User-friendly name for the server.
    pub name: String,

    /// Connection type (stdio or SSE).
    pub server_type: McpServerType,

    /// Execution configuration (command, args, URL, etc.).
    pub config: McpServerConfig,

    /// Whether tools from this server are included in chat.
    pub enabled: bool,

    /// Whether to start this server when gglib launches.
    pub auto_start: bool,

    /// Environment variables for the server process.
    pub env: Vec<McpEnvEntry>,
}

impl NewMcpServer {
    /// Create a new stdio-based MCP server.
    #[must_use]
    pub fn new_stdio(
        name: impl Into<String>,
        exe_path: impl Into<String>,
        args: Vec<String>,
        path_extra: Option<String>,
    ) -> Self {
        Self {
            name: name.into(),
            server_type: McpServerType::Stdio,
            config: McpServerConfig::stdio(exe_path, args, None, path_extra),
            enabled: true,
            auto_start: false,
            env: Vec::new(),
        }
    }

    /// Create a new SSE-based MCP server.
    #[must_use]
    pub fn new_sse(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            server_type: McpServerType::Sse,
            config: McpServerConfig::sse(url),
            enabled: true,
            auto_start: false,
            env: Vec::new(),
        }
    }

    /// Add an environment variable.
    #[must_use]
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push(McpEnvEntry::new(key, value));
        self
    }

    /// Set the working directory.
    #[must_use]
    pub fn with_working_dir(mut self, dir: impl Into<String>) -> Self {
        self.config.working_dir = Some(dir.into());
        self
    }

    /// Set auto-start.
    #[must_use]
    pub const fn with_auto_start(mut self, auto_start: bool) -> Self {
        self.auto_start = auto_start;
        self
    }

    /// Set enabled status.
    #[must_use]
    pub const fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

/// Request for updating an existing MCP server.
///
/// All fields are optional - only provided fields are updated.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateMcpServer {
    /// New user-friendly name for the server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// New connection type (stdio or SSE).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_type: Option<McpServerType>,

    /// New execution configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<McpServerConfig>,

    /// Whether tools from this server are included in chat.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,

    /// Whether to start this server when gglib launches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_start: Option<bool>,

    /// Environment variables for the server process.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<Vec<McpEnvEntry>>,
}

/// Tool definition from an MCP server.
///
/// This matches the `OpenAI` tool format used by the frontend Tool Registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    /// Tool name (function name).
    pub name: String,

    /// Human-readable description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// JSON Schema for input parameters.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<serde_json::Value>,
}

impl McpTool {
    /// Create a new tool definition.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: None,
            input_schema: None,
        }
    }

    /// Set the description.
    #[must_use]
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Set the input schema.
    #[must_use]
    pub fn with_input_schema(mut self, schema: serde_json::Value) -> Self {
        self.input_schema = Some(schema);
        self
    }
}

/// Result of a tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolResult {
    /// Whether the call succeeded.
    pub success: bool,

    /// Result data (if success).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,

    /// Error message (if failed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl McpToolResult {
    /// Create a success result.
    #[must_use]
    pub const fn success(data: serde_json::Value) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_stdio_server() {
        let server = NewMcpServer::new_stdio(
            "Test Server",
            "npx",
            vec!["-y".to_string(), "@test/mcp-server".to_string()],
            None,
        )
        .with_env("API_KEY", "secret123")
        .with_auto_start(true);

        assert_eq!(server.name, "Test Server");
        assert_eq!(server.server_type, McpServerType::Stdio);
        assert_eq!(server.config.command, Some("npx".to_string()));
        assert_eq!(server.env.len(), 1);
        assert_eq!(server.env[0].key, "API_KEY");
        assert_eq!(server.env[0].value, "secret123");
        assert!(server.auto_start);
    }

    #[test]
    fn test_new_sse_server() {
        let server = NewMcpServer::new_sse("External Server", "http://localhost:3001/sse");

        assert_eq!(server.name, "External Server");
        assert_eq!(server.server_type, McpServerType::Sse);
        assert_eq!(
            server.config.url,
            Some("http://localhost:3001/sse".to_string())
        );
        assert!(server.config.command.is_none());
    }

    #[test]
    fn test_serialization() {
        let server = NewMcpServer::new_stdio("Test", "node", vec!["server.js".to_string()], None);
        let json = serde_json::to_string(&server).unwrap();
        assert!(json.contains("\"server_type\":\"stdio\""));
        assert!(json.contains("\"name\":\"Test\""));
    }

    #[test]
    fn test_mcp_tool() {
        let tool =
            McpTool::new("get_weather").with_description("Get the current weather for a location");

        assert_eq!(tool.name, "get_weather");
        assert_eq!(
            tool.description,
            Some("Get the current weather for a location".to_string())
        );
    }

    #[test]
    fn test_tool_result() {
        let success = McpToolResult::success(serde_json::json!({"temp": 72}));
        assert!(success.success);
        assert!(success.data.is_some());

        let error = McpToolResult::error("Connection failed");
        assert!(!error.success);
        assert_eq!(error.error, Some("Connection failed".to_string()));
    }
}
