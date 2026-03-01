//! Unit tests for `FilteredToolExecutor`.
//!
//! Migrated from the inline `#[cfg(test)]` block in `src/filter.rs`.
//! Uses `MockToolExecutorPort` from `common` instead of the ad-hoc
//! `StubExecutor` struct that previously lived only in that file.

mod common;

use std::collections::HashSet;
use std::sync::Arc;

use gglib_agent::FilteredToolExecutor;
use gglib_core::ports::ToolExecutorPort;
use gglib_core::{ToolCall, ToolDefinition};
use serde_json::json;

use common::mock_tools::{MockToolBehavior, MockToolExecutorPort};

// =============================================================================
// Helpers
// =============================================================================

/// Build an inner executor exposing three named tools.
fn make_inner() -> MockToolExecutorPort {
    MockToolExecutorPort::new()
        .with_tool(
            ToolDefinition::new("allowed_tool"),
            MockToolBehavior::Immediate {
                content: "executed allowed_tool".into(),
            },
        )
        .with_tool(
            ToolDefinition::new("other_tool"),
            MockToolBehavior::Immediate {
                content: "executed other_tool".into(),
            },
        )
        .with_tool(
            ToolDefinition::new("secret_tool"),
            MockToolBehavior::Immediate {
                content: "executed secret_tool".into(),
            },
        )
}

fn make_call(name: &str) -> ToolCall {
    ToolCall {
        id: "c1".into(),
        name: name.into(),
        arguments: json!({}),
    }
}

// =============================================================================
// list_tools tests
// =============================================================================

#[tokio::test]
async fn list_tools_returns_only_allowed() {
    let allowed: HashSet<String> = ["allowed_tool".to_owned()].into();
    let f = FilteredToolExecutor::new(Arc::new(make_inner()) as Arc<dyn ToolExecutorPort>, allowed);
    let tools = f.list_tools().await;
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "allowed_tool");
}

#[tokio::test]
async fn list_tools_empty_allowlist_returns_nothing() {
    let f = FilteredToolExecutor::new(
        Arc::new(make_inner()) as Arc<dyn ToolExecutorPort>,
        HashSet::new(),
    );
    assert!(f.list_tools().await.is_empty());
}

// =============================================================================
// execute tests
// =============================================================================

#[tokio::test]
async fn execute_allowed_tool_succeeds() {
    let allowed: HashSet<String> = ["allowed_tool".to_owned()].into();
    let f = FilteredToolExecutor::new(Arc::new(make_inner()) as Arc<dyn ToolExecutorPort>, allowed);
    let result = f.execute(&make_call("allowed_tool")).await.unwrap();
    assert!(result.success);
    assert!(result.content.contains("allowed_tool"));
}

#[tokio::test]
async fn execute_disallowed_tool_returns_error() {
    let allowed: HashSet<String> = ["allowed_tool".to_owned()].into();
    let f = FilteredToolExecutor::new(Arc::new(make_inner()) as Arc<dyn ToolExecutorPort>, allowed);
    let err = f.execute(&make_call("secret_tool")).await.unwrap_err();
    assert!(err.to_string().contains("not in the allowed list"));
    assert!(err.to_string().contains("secret_tool"));
}

#[tokio::test]
async fn execute_rejects_tool_not_shown_by_list() {
    // "other_tool" exists in the inner executor but is NOT in the allowlist.
    // Simulates an adversarial model synthesising a call it was never shown.
    let allowed: HashSet<String> = ["allowed_tool".to_owned()].into();
    let f = FilteredToolExecutor::new(Arc::new(make_inner()) as Arc<dyn ToolExecutorPort>, allowed);
    let err = f.execute(&make_call("other_tool")).await.unwrap_err();
    assert!(err.to_string().contains("not in the allowed list"));
}

#[tokio::test]
async fn execute_allowed_tool_propagates_inner_error() {
    // When the inner executor returns `Err(...)` for an *allowed* tool,
    // `FilteredToolExecutor` must propagate the error unchanged rather than
    // swallowing it or converting it to a `ToolResult { success: false }`.
    let inner = MockToolExecutorPort::new().with_tool(
        ToolDefinition::new("flaky_tool"),
        MockToolBehavior::Error {
            message: "infrastructure down".into(),
        },
    );
    let allowed: HashSet<String> = ["flaky_tool".to_owned()].into();
    let f = FilteredToolExecutor::new(Arc::new(inner) as Arc<dyn ToolExecutorPort>, allowed);

    let err = f.execute(&make_call("flaky_tool")).await.unwrap_err();
    assert!(
        err.to_string().contains("infrastructure down"),
        "inner Err must be propagated verbatim; got: {err}"
    );
}
