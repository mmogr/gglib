//! A long tool's progress through [`FilteredToolExecutor`]: forwarded for an
//! allowed tool, never reached for a disallowed one.

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::domain::agent::{
    ToolCall, ToolDefinition, ToolProgressSink, ToolProgressUpdate, ToolResult, ToolStage,
};
use crate::ports::ToolExecutorPort;

use super::TOOL_NOT_AVAILABLE_MSG;
use super::filtered::FilteredToolExecutor;

/// A tool that reports one `Sampling` update, then answers; counts calls on
/// each path.
#[derive(Default)]
struct Reporting {
    plain: AtomicUsize,
    with_progress: AtomicUsize,
}

#[async_trait]
impl ToolExecutorPort for Reporting {
    async fn list_tools(&self) -> Vec<ToolDefinition> {
        vec![ToolDefinition::new("builtin:draw")]
    }

    async fn execute(&self, call: &ToolCall) -> anyhow::Result<ToolResult> {
        self.plain.fetch_add(1, Ordering::SeqCst);
        Ok(ToolResult::text(call.id.clone(), "plain", true))
    }

    async fn execute_with_progress(
        &self,
        call: &ToolCall,
        sink: &dyn ToolProgressSink,
    ) -> anyhow::Result<ToolResult> {
        self.with_progress.fetch_add(1, Ordering::SeqCst);
        sink.progress(ToolProgressUpdate {
            done: Some(2),
            total: Some(4),
            ..ToolProgressUpdate::stage(ToolStage::Sampling)
        });
        Ok(ToolResult::text(call.id.clone(), "with progress", true))
    }
}

#[derive(Default)]
struct Recorder(Mutex<Vec<ToolProgressUpdate>>);

impl ToolProgressSink for Recorder {
    fn progress(&self, update: ToolProgressUpdate) {
        self.0.lock().unwrap().push(update);
    }
}

fn call(name: &str) -> ToolCall {
    ToolCall {
        id: "c1".into(),
        name: name.into(),
        arguments: serde_json::json!({}),
    }
}

fn filter(inner: &Arc<Reporting>, allowed: &str) -> FilteredToolExecutor {
    let allowed: HashSet<String> = [allowed.to_owned()].into();
    FilteredToolExecutor::new(Arc::clone(inner) as Arc<dyn ToolExecutorPort>, allowed)
}

#[tokio::test]
async fn filtered_forwards_progress_for_an_allowed_tool() {
    let inner = Arc::new(Reporting::default());
    let sink = Recorder::default();

    let result = filter(&inner, "builtin:draw")
        .execute_with_progress(&call("builtin:draw"), &sink)
        .await
        .unwrap();

    assert_eq!(result.content, "with progress");
    assert_eq!(inner.with_progress.load(Ordering::SeqCst), 1);
    assert_eq!(inner.plain.load(Ordering::SeqCst), 0);
    let seen = sink.0.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].stage, ToolStage::Sampling);
    assert_eq!(seen[0].done, Some(2));
}

#[tokio::test]
async fn filtered_refuses_a_disallowed_tool_with_progress_without_calling_inner() {
    let inner = Arc::new(Reporting::default());
    let sink = Recorder::default();

    let err = filter(&inner, "builtin:other")
        .execute_with_progress(&call("builtin:draw"), &sink)
        .await
        .unwrap_err();

    assert!(err.to_string().contains(TOOL_NOT_AVAILABLE_MSG), "{err}");
    assert_eq!(inner.with_progress.load(Ordering::SeqCst), 0);
    assert_eq!(inner.plain.load(Ordering::SeqCst), 0);
    assert!(sink.0.lock().unwrap().is_empty());
}
