#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
#![deny(unsafe_code)]
#![deny(unused_crate_dependencies)]
// No consumer names a module of this crate — every use outside it goes through
// the re-exports below.
//
// The items inside are `pub(crate)` too. For an item in a private module,
// `unreachable_pub` and `redundant_pub_crate` demand opposite spellings;
// `redundant_pub_crate` is `allow` in `[workspace.lints.clippy]`, so it is the
// one that need not be satisfied. The note on `unreachable_pub` in the
// workspace manifest says what `dead_code` does and does not see.
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
// Re-export this crate's public types
pub use builtin::BuiltinToolExecutorAdapter;
pub use combined::CombinedToolExecutor;
pub use service::{McpServerInfo, McpService};
