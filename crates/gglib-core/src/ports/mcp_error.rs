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

    /// Tool invocation failed.
    #[error("MCP tool error: {0}")]
    ToolError(String),

    /// Configuration validation error.
    #[error("Invalid MCP configuration: {0}")]
    InvalidConfig(String),
}
