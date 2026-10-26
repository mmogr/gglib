#![doc = include_str!("README.md")]
mod device;
mod tool_index;
mod types;

pub use device::DeviceServers;

pub use tool_index::{SEARCH_RESULTS_CAP, ToolIndex, ToolSummary};
pub use types::{
    McpEnvEntry, McpLifecycle, McpServer, McpServerConfig, McpServerStatus, McpServerType, McpTool,
    McpToolResult, NewMcpServer, UpdateMcpServer,
};
