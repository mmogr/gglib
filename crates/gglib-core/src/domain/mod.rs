//! Core domain types.
//!
//! These types represent the pure domain model, independent of any
//! infrastructure concerns (database, filesystem, etc.).
//!
//! # Structure
//!
//! - `model` - Model types (`Model`, `NewModel`)
//! - `mcp` - MCP server types (`McpServer`, `NewMcpServer`, etc.)

pub mod mcp;
mod model;

// Re-export model types at the domain level for convenience
pub use model::{Model, NewModel};

// Re-export MCP types at the domain level for convenience
pub use mcp::{
    McpEnvEntry, McpServer, McpServerConfig, McpServerStatus, McpServerType, McpTool,
    McpToolResult, NewMcpServer,
};
