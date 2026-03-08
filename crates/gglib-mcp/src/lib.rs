#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
#![deny(unsafe_code)]
#![deny(unused_crate_dependencies)]

pub mod builtin;
pub mod combined;
pub(crate) mod client;
pub(crate) mod manager;
pub(crate) mod path;
pub mod resolver;
pub mod service;
pub mod tool_executor;

// Re-export domain types from core for convenience
pub use gglib_core::{
    McpEnvEntry, McpServer, McpServerConfig, McpServerStatus, McpServerType, McpTool,
    McpToolResult, NewMcpServer,
};
// Re-export DTOs from core ports
pub use gglib_core::ports::{ResolutionAttempt, ResolutionStatus};

// Re-export this crate's public types
pub use builtin::BuiltinToolExecutorAdapter;
pub use combined::CombinedToolExecutor;
pub use service::{McpServerInfo, McpService};
pub use tool_executor::McpToolExecutorAdapter;
