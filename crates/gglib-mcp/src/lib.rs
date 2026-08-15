#![doc = include_str!(concat!(env!("OUT_DIR"), "/README_GENERATED.md"))]
#![deny(unsafe_code)]
#![deny(unused_crate_dependencies)]
// No consumer names a module of this crate — every use outside it goes through
// the re-exports below.
//
// The items inside are `pub(crate)` too, as of the visibility sweep. This
// comment used to say they stayed `pub` because the two lints "cannot both be
// satisfied". That much is true — for an item in a private module they demand
// opposite spellings, and no spelling satisfies both — but it is not a reason
// to keep `pub`: `redundant_pub_crate` is `allow` in `[workspace.lints.clippy]`
// and was before this sweep, so it is the one that need not be satisfied. The
// same comment also claimed crate-internal modules are "what lets `dead_code`
// audit their contents" — see the note on `unreachable_pub` in the workspace
// manifest for why that is not how `dead_code` works either.
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
