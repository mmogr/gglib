//! Decorator that restricts a [`ToolExecutorPort`] to a named allowlist.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::agent::{ToolCall, ToolDefinition, ToolResult};
use crate::ports::ToolExecutorPort;

use super::TOOL_NOT_AVAILABLE_MSG;

// =============================================================================
// Bare-name helper
// =============================================================================

/// Return the **bare** tool name — the portion after the first `':'`.
///
/// MCP tools are qualified with a server-id prefix by
/// [`McpToolExecutorAdapter`] (e.g. `"3:read_file"` → `"read_file"`,
/// `"builtin:get_current_time"` → `"get_current_time"`).  The Director and
/// workers specify allowlists using bare names from the tool catalog; this
/// helper lets the filter match those bare names against the qualified names
/// used by the inner executor.
///
/// If the name contains no `':'` the input is returned unchanged.
pub(super) fn bare_name(qualified: &str) -> &str {
    qualified
        .find(':')
        .map_or(qualified, |pos| &qualified[pos + 1..])
}

/// Return `true` if `allowed` contains `qualified_name` **or** its bare form.
///
/// This enables the Director to emit bare names (e.g. `"browser_navigate"`) in
/// `tool_allowlist` entries while the inner executor stores qualified names
/// (e.g. `"2:browser_navigate"`).
fn is_allowed(qualified_name: &str, allowed: &HashSet<String>) -> bool {
    allowed.contains(qualified_name) || allowed.contains(bare_name(qualified_name))
}

// =============================================================================
// FilteredToolExecutor
// =============================================================================

/// Decorator that restricts a [`ToolExecutorPort`] to a named allowlist.
///
/// Both `list_tools` and `execute` enforce the allowlist:
/// - `list_tools` omits tools not in the allowlist so the LLM never learns
///   they exist.
/// - `execute` re-checks the name so an adversarially-prompted model cannot
///   invoke a tool by synthesising a call it was never shown.
///
/// # Name matching
///
/// Allowlist entries are matched against qualified tool names using
/// **both exact and bare-name matching**.  A tool named `"2:browser_navigate"`
/// in the inner executor will be included when the allowlist contains either
/// `"2:browser_navigate"` (exact) or `"browser_navigate"` (bare name after
/// stripping the `"{server-id}:"` prefix).
///
/// This lets the Director emit bare names in `tool_allowlist` entries —
/// matching the clean names shown in the `{tool_catalog}` prompt placeholder —
/// while the underlying MCP routing layer continues to use qualified names.
pub struct FilteredToolExecutor {
    inner: Arc<dyn ToolExecutorPort>,
    allowed: HashSet<String>,
}

impl FilteredToolExecutor {
    /// Wrap `inner`, exposing only tools whose names are in `allowed`.
    pub fn new(inner: Arc<dyn ToolExecutorPort>, allowed: HashSet<String>) -> Self {
        Self { inner, allowed }
    }
}

#[async_trait]
impl ToolExecutorPort for FilteredToolExecutor {
    async fn list_tools(&self) -> Vec<ToolDefinition> {
        self.inner
            .list_tools()
            .await
            .into_iter()
            .filter(|t| is_allowed(&t.name, &self.allowed))
            .collect()
    }

    /// Execute `call`, returning an error if the tool name is not in the
    /// allowlist.
    ///
    /// This is the defence-in-depth check: `list_tools` already withholds
    /// disallowed tools from the LLM, but an adversarial model might still
    /// synthesise a call by name.  Rejecting here ensures no disallowed tool
    /// can ever execute regardless of how the request was constructed.
    async fn execute(&self, call: &ToolCall) -> anyhow::Result<ToolResult> {
        if !is_allowed(&call.name, &self.allowed) {
            anyhow::bail!("tool '{}' {}", call.name, TOOL_NOT_AVAILABLE_MSG);
        }
        self.inner.execute(call).await
    }
}
