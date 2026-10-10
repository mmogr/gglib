//! A tool's own deadline against the session's timeout, the request clamp,
//! and a tool's progress reaching the loop's channel through the filter.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use gglib_core::domain::agent::{ToolProgressSink, ToolProgressUpdate, ToolStage};
use gglib_core::ports::{FilteredToolExecutor, ToolExecutorPort};
use gglib_core::{AgentConfig, AgentEvent, ToolCall, ToolDefinition, ToolResult};
use serde_json::json;
use tokio::sync::mpsc;

use super::execute_tools_parallel;

fn call(name: &str) -> ToolCall {
    ToolCall {
        id: "c1".into(),
        name: name.into(),
        arguments: json!({}),
    }
}

/// A builtin that sleeps, reports one sampling step, then answers.
struct Sleeper(Duration);

#[async_trait]
impl ToolExecutorPort for Sleeper {
    async fn list_tools(&self) -> Vec<ToolDefinition> {
        vec![]
    }

    async fn execute(&self, tc: &ToolCall) -> Result<ToolResult> {
        tokio::time::sleep(self.0).await;
        Ok(ToolResult::text(tc.id.clone(), "plain", true))
    }

    async fn execute_with_progress(
        &self,
        tc: &ToolCall,
        sink: &dyn ToolProgressSink,
    ) -> Result<ToolResult> {
        sink.progress(ToolProgressUpdate {
            pass: Some(1),
            done: Some(1),
            total: Some(4),
            ..ToolProgressUpdate::stage(ToolStage::Sampling)
        });
        tokio::time::sleep(self.0).await;
        Ok(ToolResult::text(tc.id.clone(), "drew it", true))
    }
}

fn timeout_ms(ms: u64) -> AgentConfig {
    let mut config = AgentConfig::default();
    config.tool_timeout_ms = ms;
    config
}

async fn run(
    executor: Arc<dyn ToolExecutorPort>,
    config: &AgentConfig,
    tools: &[ToolDefinition],
) -> (ToolResult, Vec<AgentEvent>) {
    let (tx, mut rx) = mpsc::channel(64);
    let mut results =
        execute_tools_parallel(&[call("builtin:draw")], &executor, config, &tx, tools).await;
    drop(tx);
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    (results.remove(0), events)
}

#[tokio::test(start_paused = true)]
async fn a_tools_own_deadline_beats_the_sessions_timeout() {
    let tools = [ToolDefinition::new("builtin:draw").with_deadline(Duration::from_secs(2))];
    let sleeper = Arc::new(Sleeper(Duration::from_millis(1_500)));

    let (with_deadline, _) = run(sleeper.clone(), &timeout_ms(1_000), &tools).await;
    let (without, _) = run(
        sleeper,
        &timeout_ms(1_000),
        &[ToolDefinition::new("builtin:draw")],
    )
    .await;

    assert!(with_deadline.success, "{}", with_deadline.content);
    assert_eq!(with_deadline.content, "drew it");
    assert!(!without.success);
    assert!(
        without.content.contains("timed out after 1000 ms"),
        "{}",
        without.content
    );
}

#[tokio::test(start_paused = true)]
async fn a_requests_timeout_still_clamps_to_sixty_seconds() {
    let config = AgentConfig::from_user_params(None, None, Some(600_000), None, None, None)
        .expect("a valid config");
    assert_eq!(config.tool_timeout_ms, 60_000);

    let (result, _) = run(
        Arc::new(Sleeper(Duration::from_secs(61))),
        &config,
        &[ToolDefinition::new("builtin:draw")],
    )
    .await;

    assert!(!result.success);
    assert!(
        result.content.contains("timed out after 60000 ms"),
        "{}",
        result.content
    );
}

#[tokio::test(start_paused = true)]
async fn a_tools_progress_reaches_the_loop_through_the_filter() {
    let allowed: HashSet<String> = ["builtin:draw".to_owned()].into();
    let filtered: Arc<dyn ToolExecutorPort> = Arc::new(FilteredToolExecutor::new(
        Arc::new(Sleeper(Duration::from_millis(10))),
        allowed,
    ));

    let (result, events) = run(filtered, &timeout_ms(1_000), &[]).await;

    assert_eq!(result.content, "drew it");
    let kinds: Vec<&str> = events
        .iter()
        .map(|e| match e {
            AgentEvent::ToolCallStart { .. } => "start",
            AgentEvent::ToolProgress { .. } => "progress",
            AgentEvent::ToolCallComplete { .. } => "complete",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["start", "progress", "complete"]);
    assert!(matches!(
        &events[1],
        AgentEvent::ToolProgress { tool_call_id, stage: ToolStage::Sampling, done: Some(1), .. }
            if tool_call_id == "c1"
    ));
}
