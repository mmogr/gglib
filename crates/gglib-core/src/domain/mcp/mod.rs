#![doc = include_str!("README.md")]
// MIGRATION: content extracted to README.md — remove this //! block after review
//! MCP (Model Context Protocol) server domain types.
//!
//! These types represent MCP servers in the system, independent of any
//! infrastructure concerns (database, process management, etc.).
//!
//! # Design
//!
//! - `McpServer` - A persisted MCP server with ID
//! - `NewMcpServer` - An MCP server to be inserted (no ID yet)
//! - `McpServerConfig` - Execution configuration (`exe_path`, args, URL, `path_extra`)
//! - `McpServerType` - Connection type (stdio or SSE)
//! - `McpServerStatus` - Runtime status (stopped, starting, running, error)
//! - `McpLifecycle` - Startup lifecycle policy (eager, lazy, manual)
//! - `McpEnvEntry` - Environment variable entry
//! - `McpTool` - Tool exposed by an MCP server
//! - `McpToolResult` - Result of a tool invocation
//! - `ToolIndex` / `ToolSummary` - Progressive-disclosure tool registry index

mod tool_index;
mod types;

pub use tool_index::{SEARCH_RESULTS_CAP, ToolIndex, ToolSummary};
pub use types::{
    McpEnvEntry, McpLifecycle, McpServer, McpServerConfig, McpServerStatus, McpServerType, McpTool,
    McpToolResult, NewMcpServer, UpdateMcpServer,
};
