//! Unit tests for `execute_tools_parallel`.
//!
//! Migrated from the inline `#[cfg(test)]` block in `src/tool_execution.rs`.
//! Uses `MockToolExecutorPort` from `common` instead of the ad-hoc `OkExecutor`
//! and `SlowExecutor` stubs that previously lived only in that file.
//!
//! The `concurrency_limited_by_semaphore` test retains a local `PeakTracker`
//! because peak-concurrency measurement requires atomic CAS and does not map
//! onto any pre-existing `MockToolBehavior` variant.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use gglib_agent::execute_tools_parallel;
use gglib_core::{AgentConfig, ToolCall, ToolDefinition, ToolResult};
use gglib_core::ports::ToolExecutorPort;
use serde_json::json;
use tokio::sync::mpsc;

use common::mock_tools::{MockToolBehavior, MockToolExecutorPort};

// =============================================================================
// Helpers
// =============================================================================

fn call(id: &str, name: &str) -> ToolCall {
    ToolCall { id: id.into(), name: name.into(), arguments: json!({}) }
}

// =============================================================================
// Tests
// =============================================================================

#[tokio::test]
async fn all_tools_return_results_in_order() {
    let (tx, mut rx) = mpsc::channel(32);
    let executor = MockToolExecutorPort::new()
        .with_tool(
            ToolDefinition::new("t"),
            MockToolBehavior::Immediate { content: "ok".into() },
        );
    let calls: Vec<ToolCall> = (0..3).map(|i| call(&format!("c{i}"), "t")).collect();

    let results = execute_tools_parallel(
        &calls,
        &(Arc::new(executor) as Arc<dyn ToolExecutorPort>),
        &AgentConfig::default(),
        &tx,
    )
    .await;

    assert_eq!(results.len(), 3);
    for (i, r) in results.iter().enumerate() {
        assert_eq!(r.tool_call_id, format!("c{i}"));
        assert!(r.success);
    }
    // Each call emits ToolCallStart + ToolCallComplete (6 total).
    let mut event_count = 0;
    while rx.try_recv().is_ok() {
        event_count += 1;
    }
    assert_eq!(event_count, 6);
}

#[tokio::test]
async fn timeout_produces_failure_result() {
    let (tx, _rx) = mpsc::channel(32);
    let executor = MockToolExecutorPort::new().with_tool(
        ToolDefinition::new("slow_tool"),
        MockToolBehavior::Delayed { millis: 1_000, content: "slow ok".into() },
    );
    let calls = vec![call("slow", "slow_tool")];
    let config = AgentConfig { tool_timeout_ms: 10, ..Default::default() };

    let results = execute_tools_parallel(
        &calls,
        &(Arc::new(executor) as Arc<dyn ToolExecutorPort>),
        &config,
        &tx,
    )
    .await;

    assert_eq!(results.len(), 1);
    assert!(!results[0].success);
    assert!(results[0].content.contains("timed out"));
}

#[tokio::test]
async fn concurrency_limited_by_semaphore() {
    /// Counts the peak number of concurrently executing tasks.
    struct PeakTracker {
        current: Arc<AtomicUsize>,
        peak: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl ToolExecutorPort for PeakTracker {
        async fn list_tools(&self) -> Vec<ToolDefinition> {
            vec![]
        }
        async fn execute(&self, call: &ToolCall) -> anyhow::Result<ToolResult> {
            let prev = self.current.fetch_add(1, Ordering::SeqCst);
            let running = prev + 1;
            let mut peak = self.peak.load(Ordering::SeqCst);
            while running > peak {
                match self.peak.compare_exchange(peak, running, Ordering::SeqCst, Ordering::SeqCst)
                {
                    Ok(_) => break,
                    Err(p) => peak = p,
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            self.current.fetch_sub(1, Ordering::SeqCst);
            Ok(ToolResult {
                tool_call_id: call.id.clone(),
                content: "ok".into(),
                success: true,
                wait_ms: 0,
                duration_ms: 20,
            })
        }
    }

    let current = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let tracker = Arc::new(PeakTracker {
        current: Arc::clone(&current),
        peak: Arc::clone(&peak),
    });
    let calls: Vec<ToolCall> =
        (0..10).map(|i| call(&format!("c{i}"), "t")).collect();
    let (tx, _rx) = mpsc::channel(64);
    let config = AgentConfig { max_parallel_tools: 3, ..Default::default() };

    execute_tools_parallel(
        &calls,
        &(tracker as Arc<dyn ToolExecutorPort>),
        &config,
        &tx,
    )
    .await;

    let observed_peak = peak.load(Ordering::SeqCst);
    assert!(
        observed_peak <= 3,
        "peak concurrency {observed_peak} exceeded limit of 3"
    );
}
