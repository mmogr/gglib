//! Built-in tool discovery endpoint.

use axum::Json;
use gglib_core::McpTool;
use gglib_mcp::BuiltinToolExecutorAdapter;

/// Return the definitions for all built-in tools (without server prefix).
///
/// The frontend uses this endpoint to register built-in tools into the
/// `ToolRegistry` under the `"builtin"` source, replacing the stale
/// TypeScript-defined list.  No state is needed — built-ins are static.
pub async fn list_builtin_tools() -> Json<Vec<McpTool>> {
    Json(BuiltinToolExecutorAdapter::bare_definitions())
}
