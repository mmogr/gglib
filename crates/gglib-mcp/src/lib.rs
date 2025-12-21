#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
#![deny(unsafe_code)]
#![deny(unused_crate_dependencies)]

pub(crate) mod client;
pub(crate) mod manager;
pub(crate) mod path;
pub mod resolver;
pub mod service;

// Re-export domain types from core for convenience
pub use gglib_core::{
    McpEnvEntry, McpServer, McpServerConfig, McpServerStatus, McpServerType, McpTool,
    McpToolResult, NewMcpServer,
};
// Re-export DTOs from core ports
pub use gglib_core::ports::{ResolutionAttempt, ResolutionStatus};

// Re-export this crate's public types
pub use service::{McpServerInfo, McpService};
