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
// Shared rejection message
// =============================================================================

/// Sentinel phrase embedded in every tool-rejection error produced by this
/// module.  Both [`EmptyToolExecutor`] and [`FilteredToolExecutor`] use this
/// constant so tests can assert on `error_string.contains(TOOL_NOT_AVAILABLE_MSG)`
/// without depending on the surrounding format string.
pub const TOOL_NOT_AVAILABLE_MSG: &str = "is not available in this session";

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
#[derive(Debug, Default, Clone)]
pub(crate) struct EmptyToolExecutor;

#[async_trait]
impl ToolExecutorPort for EmptyToolExecutor {
    async fn list_tools(&self) -> Vec<ToolDefinition> {
        vec![]
    }

    async fn execute(&self, call: &ToolCall) -> anyhow::Result<ToolResult> {
        anyhow::bail!("tool '{}' {}", call.name, TOOL_NOT_AVAILABLE_MSG);
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
/// (e.g. `"3:read_file"`).  The `tool_filter` values forwarded from the
/// frontend should therefore use those same qualified names.
pub(crate) struct FilteredToolExecutor {
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
            anyhow::bail!("tool '{}' {}", call.name, TOOL_NOT_AVAILABLE_MSG);
        }
        self.inner.execute(call).await
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // Minimal stub executor
    // ------------------------------------------------------------------

    struct StubExecutor {
        tools: Vec<ToolDefinition>,
        /// When `Some(name)` matches the call, return `Err(message)`.
        error_tool: Option<(&'static str, &'static str)>,
    }

    impl StubExecutor {
        fn new(names: &[&'static str]) -> Self {
            Self {
                tools: names.iter().copied().map(ToolDefinition::new).collect(),
                error_tool: None,
            }
        }

        fn with_error(mut self, name: &'static str, msg: &'static str) -> Self {
            self.error_tool = Some((name, msg));
            self
        }
    }

    #[async_trait::async_trait]
    impl ToolExecutorPort for StubExecutor {
        async fn list_tools(&self) -> Vec<ToolDefinition> {
            self.tools.clone()
        }

        async fn execute(&self, call: &ToolCall) -> anyhow::Result<ToolResult> {
            if let Some((name, msg)) = self.error_tool {
                if call.name == name {
                    anyhow::bail!("{msg}");
                }
            }
            Ok(ToolResult {
                tool_call_id: call.id.clone(),
                content: format!("executed {}", call.name),
                success: true,
            })
        }
    }

    fn make_call(name: &str) -> ToolCall {
        ToolCall {
            id: "c1".into(),
            name: name.into(),
            arguments: serde_json::json!({}),
        }
    }

    // ------------------------------------------------------------------
    // list_tools
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn list_tools_returns_only_allowed() {
        let allowed: HashSet<String> = ["allowed_tool".to_owned()].into();
        let f = FilteredToolExecutor::new(
            Arc::new(StubExecutor::new(&[
                "allowed_tool",
                "other_tool",
                "secret_tool",
            ])) as Arc<dyn ToolExecutorPort>,
            allowed,
        );
        let tools = f.list_tools().await;
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "allowed_tool");
    }

    #[tokio::test]
    async fn list_tools_empty_allowlist_returns_nothing() {
        let f = FilteredToolExecutor::new(
            Arc::new(StubExecutor::new(&["allowed_tool"])) as Arc<dyn ToolExecutorPort>,
            HashSet::new(),
        );
        assert!(f.list_tools().await.is_empty());
    }

    // ------------------------------------------------------------------
    // execute
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn execute_allowed_tool_succeeds() {
        let allowed: HashSet<String> = ["allowed_tool".to_owned()].into();
        let f = FilteredToolExecutor::new(
            Arc::new(StubExecutor::new(&["allowed_tool"])) as Arc<dyn ToolExecutorPort>,
            allowed,
        );
        let result = f.execute(&make_call("allowed_tool")).await.unwrap();
        assert!(result.success);
        assert!(result.content.contains("allowed_tool"));
    }

    #[tokio::test]
    async fn execute_disallowed_tool_returns_error() {
        let allowed: HashSet<String> = ["allowed_tool".to_owned()].into();
        let f = FilteredToolExecutor::new(
            Arc::new(StubExecutor::new(&["allowed_tool", "secret_tool"]))
                as Arc<dyn ToolExecutorPort>,
            allowed,
        );
        let err = f.execute(&make_call("secret_tool")).await.unwrap_err();
        assert!(err.to_string().contains(TOOL_NOT_AVAILABLE_MSG));
        assert!(err.to_string().contains("secret_tool"));
    }

    #[tokio::test]
    async fn execute_rejects_tool_not_shown_by_list() {
        // "other_tool" exists in the inner executor but is NOT in the allowlist.
        // Simulates an adversarial model synthesising a call it was never shown.
        let allowed: HashSet<String> = ["allowed_tool".to_owned()].into();
        let f = FilteredToolExecutor::new(
            Arc::new(StubExecutor::new(&["allowed_tool", "other_tool"]))
                as Arc<dyn ToolExecutorPort>,
            allowed,
        );
        let err = f.execute(&make_call("other_tool")).await.unwrap_err();
        assert!(err.to_string().contains(TOOL_NOT_AVAILABLE_MSG));
    }

    #[tokio::test]
    async fn execute_allowed_tool_propagates_inner_error() {
        // When the inner executor returns Err for an *allowed* tool,
        // FilteredToolExecutor must propagate the error unchanged.
        let inner =
            StubExecutor::new(&["flaky_tool"]).with_error("flaky_tool", "infrastructure down");
        let allowed: HashSet<String> = ["flaky_tool".to_owned()].into();
        let f = FilteredToolExecutor::new(Arc::new(inner) as Arc<dyn ToolExecutorPort>, allowed);
        let err = f.execute(&make_call("flaky_tool")).await.unwrap_err();
        assert!(
            err.to_string().contains("infrastructure down"),
            "inner Err must be propagated verbatim; got: {err}"
        );
    }

    // ------------------------------------------------------------------
    // EmptyToolExecutor
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn empty_executor_list_tools_returns_empty() {
        let e = EmptyToolExecutor;
        assert!(
            e.list_tools().await.is_empty(),
            "EmptyToolExecutor must expose no tools"
        );
    }

    #[tokio::test]
    async fn empty_executor_execute_returns_error() {
        let e = EmptyToolExecutor;
        let call = make_call("any_tool");
        let err = e.execute(&call).await.unwrap_err();
        assert!(
            err.to_string().contains(TOOL_NOT_AVAILABLE_MSG),
            "EmptyToolExecutor::execute must emit TOOL_NOT_AVAILABLE_MSG; got: {err}"
        );
        assert!(
            err.to_string().contains("any_tool"),
            "error message must include the tool name; got: {err}"
        );
    }

    #[test]
    fn empty_executor_derives_are_sound() {
        // Exercises the Debug, Default, and Clone derives to ensure they
        // compile and produce values that satisfy basic sanity checks.
        let a = EmptyToolExecutor::default();
        let b = a.clone();
        // Debug must not panic.
        let _ = format!("{a:?}");
        let _ = format!("{b:?}");
    }
}
