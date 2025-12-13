//! MCP server lifecycle events.

use serde::{Deserialize, Serialize};

use super::AppEvent;
use crate::ports::McpErrorInfo;

/// Summary of an MCP server for event payloads.
///
/// This is a lightweight representation for events — not the full `McpServer`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerSummary {
    /// Database ID of the MCP server.
    pub id: i64,
    /// User-friendly name of the server.
    pub name: String,
    /// Server type (stdio or sse).
    pub server_type: String,
}

impl McpServerSummary {
    /// Create a new MCP server summary.
    pub fn new(id: i64, name: impl Into<String>, server_type: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            server_type: server_type.into(),
        }
    }
}

impl AppEvent {
    /// Create an MCP server added event.
    pub const fn mcp_server_added(server: McpServerSummary) -> Self {
        Self::McpServerAdded { server }
    }

    /// Create an MCP server removed event.
    pub const fn mcp_server_removed(server_id: i64) -> Self {
        Self::McpServerRemoved { server_id }
    }

    /// Create an MCP server started event.
    pub fn mcp_server_started(server_id: i64, server_name: impl Into<String>) -> Self {
        Self::McpServerStarted {
            server_id,
            server_name: server_name.into(),
        }
    }

    /// Create an MCP server stopped event.
    pub fn mcp_server_stopped(server_id: i64, server_name: impl Into<String>) -> Self {
        Self::McpServerStopped {
            server_id,
            server_name: server_name.into(),
        }
    }

    /// Create an MCP server error event.
    pub const fn mcp_server_error(error: McpErrorInfo) -> Self {
        Self::McpServerError { error }
    }
}
