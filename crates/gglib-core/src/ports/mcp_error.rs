//! MCP service error types.
//!
//! This module defines service-level errors for MCP operations.

use thiserror::Error;

use super::McpRepositoryError;

/// Domain-specific errors for MCP service operations.
///
/// This error type wraps repository errors and adds service-level failure modes
/// without leaking infrastructure details (OS process errors, SQL errors, etc.).
#[derive(Debug, Error)]
pub enum McpServiceError {
    /// Repository operation failed.
    #[error(transparent)]
    Repository(#[from] McpRepositoryError),

    /// Server process failed to start.
    #[error("Failed to start MCP server: {0}")]
    StartFailed(String),

    /// Server process failed to stop.
    #[error("Failed to stop MCP server: {0}")]
    StopFailed(String),

    /// Server is not running (e.g., when trying to stop).
    #[error("MCP server not running: {0}")]
    NotRunning(String),

    /// Protocol error (JSON-RPC communication failure).
    #[error("MCP protocol error: {0}")]
    Protocol(String),

    /// Tool invocation failed.
    #[error("MCP tool error: {0}")]
    ToolError(String),

    /// Configuration validation error.
    #[error("Invalid MCP configuration: {0}")]
    InvalidConfig(String),

    /// Internal service error.
    #[error("Internal MCP error: {0}")]
    Internal(String),
}

/// User-safe error information for MCP events.
///
/// This type is used in `AppEvent::McpServerError` to provide error details
/// that are safe to display to users (no raw process/SQL errors).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpErrorInfo {
    /// ID of the MCP server (if known).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_id: Option<i64>,

    /// Name of the MCP server.
    pub server_name: String,

    /// User-friendly error message.
    pub message: String,

    /// Error category for UI handling.
    pub category: McpErrorCategory,
}

/// Categories of MCP errors for UI handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpErrorCategory {
    /// Server process lifecycle error.
    Process,
    /// Protocol communication error.
    Protocol,
    /// Tool invocation error.
    Tool,
    /// Configuration error.
    Configuration,
    /// Unknown/internal error.
    Unknown,
}

impl McpErrorInfo {
    /// Create error info for a process error.
    pub fn process(
        server_id: Option<i64>,
        server_name: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            server_id,
            server_name: server_name.into(),
            message: message.into(),
            category: McpErrorCategory::Process,
        }
    }

    /// Create error info for a protocol error.
    pub fn protocol(
        server_id: Option<i64>,
        server_name: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            server_id,
            server_name: server_name.into(),
            message: message.into(),
            category: McpErrorCategory::Protocol,
        }
    }

    /// Create error info for a tool error.
    pub fn tool(
        server_id: Option<i64>,
        server_name: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            server_id,
            server_name: server_name.into(),
            message: message.into(),
            category: McpErrorCategory::Tool,
        }
    }
}

impl From<&McpServiceError> for McpErrorCategory {
    fn from(error: &McpServiceError) -> Self {
        match error {
            McpServiceError::Repository(_) | McpServiceError::Internal(_) => Self::Unknown,
            McpServiceError::StartFailed(_)
            | McpServiceError::StopFailed(_)
            | McpServiceError::NotRunning(_) => Self::Process,
            McpServiceError::Protocol(_) => Self::Protocol,
            McpServiceError::ToolError(_) => Self::Tool,
            McpServiceError::InvalidConfig(_) => Self::Configuration,
        }
    }
}
