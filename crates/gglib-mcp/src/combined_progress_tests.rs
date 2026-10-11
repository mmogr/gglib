//! A builtin's progress through [`CombinedToolExecutor`] and a filter over
//! it, as the daemon composes them.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use gglib_core::domain::agent::{ToolProgressSink, ToolProgressUpdate, ToolStage};
use gglib_core::ports::{FilteredToolExecutor, ToolExecutorPort};
use gglib_core::{ToolCall, ToolDefinition, ToolResult};

use super::CombinedToolExecutor;

/// Reports one `Loading` update on the progress path and answers with
/// `name`; answers `plain` on the other.
struct Reporting(&'static str);

#[async_trait]
impl ToolExecutorPort for Reporting {
    async fn list_tools(&self) -> Vec<ToolDefinition> {
        vec![]
    }

    async fn execute(&self, call: &ToolCall) -> anyhow::Result<ToolResult> {
        Ok(ToolResult::text(call.id.clone(), "plain", true))
    }

    async fn execute_with_progress(
        &self,
        call: &ToolCall,
        sink: &dyn ToolProgressSink,
    ) -> anyhow::Result<ToolResult> {
        sink.progress(ToolProgressUpdate::stage(ToolStage::Loading));
        Ok(ToolResult::text(call.id.clone(), self.0, true))
    }
}

#[derive(Default)]
struct Recorder(Mutex<Vec<ToolStage>>);

impl ToolProgressSink for Recorder {
    fn progress(&self, update: ToolProgressUpdate) {
        self.0.lock().unwrap().push(update.stage);
    }
}

fn combined() -> CombinedToolExecutor {
    CombinedToolExecutor {
        builtin: Arc::new(Reporting("builtin")),
        mcp: Arc::new(Reporting("mcp")),
    }
}

fn call(name: &str) -> ToolCall {
    ToolCall {
        id: "c1".into(),
        name: name.into(),
        arguments: serde_json::json!({}),
    }
}

#[tokio::test]
async fn a_builtins_progress_passes_through_a_filter_over_the_combined_executor() {
    let allowed: HashSet<String> = ["builtin:draw".to_owned()].into();
    let filtered = FilteredToolExecutor::new(Arc::new(combined()), allowed);
    let sink = Recorder::default();

    let result = filtered
        .execute_with_progress(&call("builtin:draw"), &sink)
        .await
        .unwrap();

    assert_eq!(result.content, "builtin");
    assert_eq!(*sink.0.lock().unwrap(), [ToolStage::Loading]);
}

#[tokio::test]
async fn the_progress_path_routes_by_prefix_as_execute_does() {
    let sink = Recorder::default();
    let executor = combined();

    let mcp = executor
        .execute_with_progress(&call("3:read_file"), &sink)
        .await
        .unwrap();
    let bare = executor
        .execute_with_progress(&call("draw"), &sink)
        .await
        .unwrap_err();

    assert_eq!(mcp.content, "mcp");
    assert!(bare.to_string().contains("no recognised prefix"), "{bare}");
}
