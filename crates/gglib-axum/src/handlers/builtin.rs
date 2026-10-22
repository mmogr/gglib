//! Built-in tool discovery endpoint.

use axum::Json;
use gglib_core::McpTool;
use gglib_mcp::BuiltinToolExecutorAdapter;

/// Return the definitions of the built-in tools a person can switch on
/// (without server prefix): those that need nothing from the run.
///
/// The frontend uses this endpoint to register built-in tools into the
/// `ToolRegistry` under the `"builtin"` source, replacing the stale
/// TypeScript-defined list. The file tools and `generate_image` are not
/// listed: the server decides when a run is offered them. No state is
/// needed, as the list is static.
pub(crate) async fn list_builtin_tools() -> Json<Vec<McpTool>> {
    Json(BuiltinToolExecutorAdapter::bare_definitions())
}
