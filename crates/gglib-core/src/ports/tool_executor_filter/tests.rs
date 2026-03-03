use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::agent::{ToolCall, ToolDefinition, ToolResult};
use crate::ports::ToolExecutorPort;

use super::TOOL_NOT_AVAILABLE_MSG;
use super::empty::EmptyToolExecutor;
use super::filtered::FilteredToolExecutor;

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

#[async_trait]
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
        Arc::new(StubExecutor::new(&["allowed_tool", "secret_tool"])) as Arc<dyn ToolExecutorPort>,
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
        Arc::new(StubExecutor::new(&["allowed_tool", "other_tool"])) as Arc<dyn ToolExecutorPort>,
        allowed,
    );
    let err = f.execute(&make_call("other_tool")).await.unwrap_err();
    assert!(err.to_string().contains(TOOL_NOT_AVAILABLE_MSG));
}

#[tokio::test]
async fn execute_allowed_tool_propagates_inner_error() {
    // When the inner executor returns Err for an *allowed* tool,
    // FilteredToolExecutor must propagate the error unchanged.
    let inner = StubExecutor::new(&["flaky_tool"]).with_error("flaky_tool", "infrastructure down");
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
