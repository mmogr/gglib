use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Result;
use async_trait::async_trait;
use gglib_core::ports::ToolExecutorPort;
use gglib_core::{AgentConfig, ToolCall, ToolDefinition, ToolResult};
use serde_json::json;
use tokio::sync::mpsc;

use super::execute_tools_parallel;

fn call(id: &str, name: &str) -> ToolCall {
    ToolCall {
        id: id.into(),
        name: name.into(),
        arguments: json!({}),
    }
}

// ---- Minimal test executors -------------------------------------------------

struct OkExecutor;

#[async_trait]
impl ToolExecutorPort for OkExecutor {
    async fn list_tools(&self) -> Vec<ToolDefinition> {
        vec![]
    }
    async fn execute(&self, tc: &ToolCall) -> Result<ToolResult> {
        Ok(ToolResult {
            tool_call_id: tc.id.clone(),
            content: "ok".into(),
            success: true,
        })
    }
}

struct SlowExecutor {
    millis: u64,
}

#[async_trait]
impl ToolExecutorPort for SlowExecutor {
    async fn list_tools(&self) -> Vec<ToolDefinition> {
        vec![]
    }
    async fn execute(&self, tc: &ToolCall) -> Result<ToolResult> {
        tokio::time::sleep(std::time::Duration::from_millis(self.millis)).await;
        Ok(ToolResult {
            tool_call_id: tc.id.clone(),
            content: "slow ok".into(),
            success: true,
        })
    }
}

// ---- Tests ------------------------------------------------------------------

#[tokio::test]
async fn all_tools_return_results_in_order() {
    let (tx, mut rx) = mpsc::channel(32);
    let executor: Arc<dyn ToolExecutorPort> = Arc::new(OkExecutor);
    let calls: Vec<ToolCall> = (0..3).map(|i| call(&format!("c{i}"), "t")).collect();

    let results = execute_tools_parallel(&calls, &executor, &AgentConfig::default(), &tx, &[]).await;

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
    let executor: Arc<dyn ToolExecutorPort> = Arc::new(SlowExecutor { millis: 1_000 });
    let calls = vec![call("slow", "slow_tool")];
    let mut config = AgentConfig::default();
    config.tool_timeout_ms = 10;

    let results = execute_tools_parallel(&calls, &executor, &config, &tx, &[]).await;

    assert_eq!(results.len(), 1);
    assert!(!results[0].success);
    assert!(results[0].content.contains("timed out"));
}

#[tokio::test]
async fn concurrency_limited_by_semaphore() {
    /// Tracks the peak number of concurrently executing tool calls.
    struct PeakTracker {
        current: Arc<AtomicUsize>,
        peak: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl ToolExecutorPort for PeakTracker {
        async fn list_tools(&self) -> Vec<ToolDefinition> {
            vec![]
        }
        async fn execute(&self, tc: &ToolCall) -> Result<ToolResult> {
            let prev = self.current.fetch_add(1, Ordering::SeqCst);
            let running = prev + 1;
            let mut peak = self.peak.load(Ordering::SeqCst);
            while running > peak {
                match self
                    .peak
                    .compare_exchange(peak, running, Ordering::SeqCst, Ordering::SeqCst)
                {
                    Ok(_) => break,
                    Err(p) => peak = p,
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            self.current.fetch_sub(1, Ordering::SeqCst);
            Ok(ToolResult {
                tool_call_id: tc.id.clone(),
                content: "ok".into(),
                success: true,
            })
        }
    }

    let current = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let tracker: Arc<dyn ToolExecutorPort> = Arc::new(PeakTracker {
        current: Arc::clone(&current),
        peak: Arc::clone(&peak),
    });
    let calls: Vec<ToolCall> = (0..10).map(|i| call(&format!("c{i}"), "t")).collect();
    let (tx, _rx) = mpsc::channel(64);
    let mut config = AgentConfig::default();
    config.max_parallel_tools = 3;

    execute_tools_parallel(&calls, &tracker, &config, &tx, &[]).await;

    let observed_peak = peak.load(Ordering::SeqCst);
    assert!(
        observed_peak <= 3,
        "peak concurrency {observed_peak} exceeded limit of 3"
    );
}

/// An executor that always returns `Err(...)`, simulating an infrastructure
/// failure (e.g. network down, MCP server unreachable).
struct ErrorExecutor;

#[async_trait]
impl ToolExecutorPort for ErrorExecutor {
    async fn list_tools(&self) -> Vec<ToolDefinition> {
        vec![]
    }
    async fn execute(&self, _tc: &ToolCall) -> Result<ToolResult> {
        anyhow::bail!("infrastructure down")
    }
}

#[tokio::test]
async fn executor_error_produces_failure_result() {
    let (tx, _rx) = mpsc::channel(32);
    let executor: Arc<dyn ToolExecutorPort> = Arc::new(ErrorExecutor);
    let calls = vec![call("err1", "broken_tool")];

    let results = execute_tools_parallel(&calls, &executor, &AgentConfig::default(), &tx, &[]).await;

    assert_eq!(results.len(), 1);
    assert!(!results[0].success, "result should indicate failure");
    assert!(
        results[0].content.contains("infrastructure down"),
        "error message should propagate: got {:?}",
        results[0].content,
    );
}
