//! [`FilteredToolExecutor`] — a decorator that restricts a [`ToolExecutorPort`]
//! to a named allowlist of tools.
//!
//! Used by both the Axum HTTP handler (`gglib-axum`) and the CLI agent handler
//! (`gglib-cli`) wherever the caller supplies a `tool_filter` / `--tools`
//! option.  Keeping this decorator in the `gglib-agent` crate avoids
//! duplicating it across consumers.

use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use gglib_core::domain::agent::{ToolCall, ToolDefinition, ToolResult};
use gglib_core::ports::ToolExecutorPort;

// =============================================================================
// FilteredToolExecutor
// =============================================================================

/// Decorator that restricts a [`ToolExecutorPort`] to a named allowlist.
///
/// `list_tools` returns only the tools whose names appear in `allowed`.
/// `execute` is delegated unchanged — the LLM will only request tools it was
/// told about via `list_tools`, so the allowlist is effectively enforced at
/// the listing step.
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

    async fn execute(&self, call: &ToolCall) -> anyhow::Result<ToolResult> {
        self.inner.execute(call).await
    }
}
