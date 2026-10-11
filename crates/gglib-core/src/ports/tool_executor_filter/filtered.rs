//! Decorator that restricts a [`ToolExecutorPort`] to a named allowlist.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::agent::{ToolCall, ToolDefinition, ToolProgressSink, ToolResult};
use crate::ports::ToolExecutorPort;

use super::TOOL_NOT_AVAILABLE_MSG;

// =============================================================================
// Bare-name helper
// =============================================================================

/// Return the **bare** tool name — the portion after the first `':'`.
///
/// MCP tools are qualified with a server-id prefix by
/// `McpToolExecutorAdapter` (e.g. `"3:read_file"` → `"read_file"`,
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

/// What a builtin's qualified name starts with. No MCP server has this id:
/// its ids are numbers, and the name `builtin` is refused to a server.
const BUILTIN_PREFIX: &str = "builtin:";

/// Return `true` if `allowed` contains `qualified_name` **or** its bare form.
///
/// This enables the Director to emit bare names (e.g. `"browser_navigate"`) in
/// `tool_allowlist` entries while the inner executor stores qualified names
/// (e.g. `"2:browser_navigate"`).
///
/// An entry that starts with `builtin:` names a builtin and nothing else, so
/// it matches only as the whole qualified name. A tool's bare form that
/// itself starts with `builtin:` is an MCP server's tool whose own name has
/// the colon (`3:builtin:generate_image`), and it never matches by that
/// form: an allowlist of `builtin:generate_image` would otherwise hand it to
/// a caller who was given the builtin alone.
fn is_allowed(qualified_name: &str, allowed: &HashSet<String>) -> bool {
    if allowed.contains(qualified_name) {
        return true;
    }
    let bare = bare_name(qualified_name);
    !bare.starts_with(BUILTIN_PREFIX) && allowed.contains(bare)
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

    /// Refuse a call whose tool is not in the allowlist.
    fn check(&self, call: &ToolCall) -> anyhow::Result<()> {
        if !is_allowed(&call.name, &self.allowed) {
            anyhow::bail!("tool '{}' {}", call.name, TOOL_NOT_AVAILABLE_MSG);
        }
        Ok(())
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
        self.check(call)?;
        self.inner.execute(call).await
    }

    /// The same allowlist check as [`execute`](Self::execute), then the
    /// inner executor's own `execute_with_progress`, so a long tool's
    /// progress passes through the filter and a disallowed tool still never
    /// runs.
    async fn execute_with_progress(
        &self,
        call: &ToolCall,
        sink: &dyn ToolProgressSink,
    ) -> anyhow::Result<ToolResult> {
        self.check(call)?;
        self.inner.execute_with_progress(call, sink).await
    }
}
