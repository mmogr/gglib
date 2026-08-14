#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
#![deny(unsafe_code)]
#![deny(unused_crate_dependencies)]

// No consumer names a module of this crate — every use outside it goes through
// the re-exports below. Keeping the modules crate-internal is what lets
// `unreachable_pub` and then `dead_code` audit their contents.
pub(crate) mod builtin;
pub(crate) mod client;
pub(crate) mod combined;
pub(crate) mod manager;
pub(crate) mod path;
pub(crate) mod resolver;
pub(crate) mod service;
pub(crate) mod tool_executor;

// Re-export domain types from core for convenience
pub use gglib_core::{
    McpEnvEntry, McpLifecycle, McpServer, McpServerConfig, McpServerStatus, McpServerType, McpTool,
    McpToolResult, NewMcpServer,
};
// Re-export DTOs from core ports
pub use gglib_core::ports::{ResolutionAttempt, ResolutionStatus};

// Re-export this crate's public types
pub use builtin::BuiltinToolExecutorAdapter;
pub use combined::CombinedToolExecutor;
pub use service::{McpServerInfo, McpService};
pub use tool_executor::McpToolExecutorAdapter;
