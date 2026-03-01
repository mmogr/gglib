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

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use gglib_core::ports::ToolExecutorPort;
    use gglib_core::{ToolCall, ToolDefinition, ToolResult};
    use serde_json::json;

    use super::*;

    // ---- Minimal stub executor -------------------------------------------

    struct StubExecutor;

    #[async_trait]
    impl ToolExecutorPort for StubExecutor {
        async fn list_tools(&self) -> Vec<ToolDefinition> {
            vec![
                ToolDefinition::new("allowed_tool"),
                ToolDefinition::new("other_tool"),
                ToolDefinition::new("secret_tool"),
            ]
        }
        async fn execute(&self, call: &ToolCall) -> anyhow::Result<ToolResult> {
            Ok(ToolResult {
                tool_call_id: call.id.clone(),
                content: format!("executed {}", call.name),
                success: true,
                duration_ms: 0,
                wait_ms: 0,
            })
        }
    }

    fn make_call(name: &str) -> ToolCall {
        ToolCall {
            id: "c1".into(),
            name: name.into(),
            arguments: json!({}),
        }
    }

    // ---- list_tools tests -----------------------------------------------

    #[tokio::test]
    async fn list_tools_returns_only_allowed() {
        let allowed = ["allowed_tool".to_owned()].into();
        let f = FilteredToolExecutor::new(Arc::new(StubExecutor), allowed);
        let tools = f.list_tools().await;
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "allowed_tool");
    }

    #[tokio::test]
    async fn list_tools_empty_allowlist_returns_nothing() {
        let f = FilteredToolExecutor::new(Arc::new(StubExecutor), HashSet::new());
        assert!(f.list_tools().await.is_empty());
    }

    // ---- execute tests --------------------------------------------------

    #[tokio::test]
    async fn execute_allowed_tool_succeeds() {
        let allowed = ["allowed_tool".to_owned()].into();
        let f = FilteredToolExecutor::new(Arc::new(StubExecutor), allowed);
        let result = f.execute(&make_call("allowed_tool")).await.unwrap();
        assert!(result.success);
        assert!(result.content.contains("allowed_tool"));
    }

    #[tokio::test]
    async fn execute_disallowed_tool_returns_error() {
        let allowed = ["allowed_tool".to_owned()].into();
        let f = FilteredToolExecutor::new(Arc::new(StubExecutor), allowed);
        let err = f.execute(&make_call("secret_tool")).await.unwrap_err();
        assert!(err.to_string().contains("not in the allowed list"));
        assert!(err.to_string().contains("secret_tool"));
    }

    #[tokio::test]
    async fn execute_rejects_tool_not_shown_by_list() {
        // "other_tool" exists in the inner executor but is NOT in the allowlist.
        // Simulates an adversarial model synthesising a call it was never told about.
        let allowed = ["allowed_tool".to_owned()].into();
        let f = FilteredToolExecutor::new(Arc::new(StubExecutor), allowed);
        let err = f.execute(&make_call("other_tool")).await.unwrap_err();
        assert!(err.to_string().contains("not in the allowed list"));
    }
}
