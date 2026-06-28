#![doc = include_str!("README.md")]
mod tool_index;
mod types;

pub use tool_index::{SEARCH_RESULTS_CAP, ToolIndex, ToolSummary};
pub use types::{
    McpEnvEntry, McpLifecycle, McpServer, McpServerConfig, McpServerStatus, McpServerType, McpTool,
    McpToolResult, NewMcpServer, UpdateMcpServer,
};
