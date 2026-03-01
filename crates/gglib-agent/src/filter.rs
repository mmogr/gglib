//! [`FilteredToolExecutor`] — a decorator that restricts a [`ToolExecutorPort`]
//! to a named allowlist of tools.
//!
//! # Architectural placement
//!
//! This is a *concrete implementation* (a decorator), not a pure port or domain
//! type, so it lives here in `gglib-agent` (the orchestration layer) rather
//! than in `gglib-core` (which contains only traits and domain models).
//! Both downstream consumers — the Axum HTTP handler (`gglib-axum`) and the
//! CLI agent handler (`gglib-cli`) — already depend on `gglib-agent`, so
//! keeping the decorator here is DRY with zero extra dependency edges.
//!
//! # Security model
//!
//! The allowlist is enforced on **both** `list_tools` (so the LLM only sees
//! permitted tools) and `execute` (so an adversarially-prompted model that
//! synthesises a call for a tool it was never told about cannot bypass the
//! filter).

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::ports::ToolExecutorPort;
use gglib_core::{ToolCall, ToolDefinition, ToolResult};

// =============================================================================
// EmptyToolExecutor
// =============================================================================

/// A [`ToolExecutorPort`] that exposes no tools and rejects all execution.
///
/// Used by [`super::agent_loop::AgentLoop::build`] when the caller passes
/// `Some(empty_set)` as the `tool_filter` argument.  An empty allowlist means
/// *"expose nothing"*, not *"expose everything"* — this executor enforces that
/// contract on both `list_tools` (LLM sees no tools) and `execute` (defence in
/// depth: any synthesised call is rejected).
pub(crate) struct EmptyToolExecutor;

#[async_trait]
impl ToolExecutorPort for EmptyToolExecutor {
    async fn list_tools(&self) -> Vec<ToolDefinition> {
        vec![]
    }

    async fn execute(&self, call: &ToolCall) -> anyhow::Result<ToolResult> {
        anyhow::bail!("tool '{}' is not available in this session", call.name);
    }
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
/// The allowlist is compared against the **exact names returned by the inner
/// executor's `list_tools`**.  When the inner executor is
/// [`McpToolExecutorAdapter`], names are qualified with a server-id prefix
/// (e.g. `"3__read_file"`).  The `tool_filter` values forwarded from the
/// frontend should therefore use those same qualified names.
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
            .filter(|t| self.allowed.contains(&t.name))
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
        if !self.allowed.contains(&call.name) {
            anyhow::bail!("tool '{}' is not in the allowed list", call.name);
        }
        self.inner.execute(call).await
    }
}
