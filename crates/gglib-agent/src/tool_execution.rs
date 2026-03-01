//! Parallel tool execution with bounded concurrency and per-tool timeout.
//!
//! This module is the Rust port of `executeToolBatch` from
//! `src/hooks/useGglibRuntime/toolBatchExecution.ts`.
//!
//! # Behaviour
//!
//! - All tool calls in a batch are dispatched concurrently via a
//!   [`tokio::task::JoinSet`].  When the future returned by
//!   [`execute_tools_parallel`] is dropped (e.g. because `AgentTaskGuard`
//!   aborts the parent agent task on client disconnect), the `JoinSet` is
//!   dropped and every in-flight sub-task is cancelled — no resource leak.
//! - A [`tokio::sync::Semaphore`] caps the number of *simultaneously running*
//!   tool calls at [`AgentConfig::max_parallel_tools`].
//! - Each call is wrapped in a [`tokio::time::timeout`] capped at
//!   [`AgentConfig::tool_timeout_ms`].
//! - A timeout or `Err` from the executor produces a
//!   `ToolResult { success: false, … }` rather than aborting the batch —
//!   the LLM can observe the failure and decide how to proceed.
//! - [`AgentEvent::ToolCallStart`] and [`AgentEvent::ToolCallComplete`] are
//!   sent on `tx` before and after each call.

use std::sync::Arc;
use std::time::{Duration, Instant};

use gglib_core::ports::ToolExecutorPort;
use gglib_core::{AgentConfig, AgentEvent, ToolCall, ToolResult};
use tokio::sync::{Semaphore, mpsc};
use tokio::task::JoinSet;
use tracing::warn;

use gglib_core::elapsed_ms;

// =============================================================================
// Private helpers
// =============================================================================

/// Execute a single tool call, respecting the shared concurrency semaphore and
/// a per-call timeout.
///
/// Emits [`AgentEvent::ToolCallStart`] after acquiring the semaphore permit and
/// [`AgentEvent::ToolCallComplete`] when the call finishes (whether it succeeded,
/// errored, or timed out).  Send failures on `tx` are silently ignored — the
/// SSE client may have disconnected.
async fn execute_single_tool(
    tc: ToolCall,
    executor: Arc<dyn ToolExecutorPort>,
    sem: Arc<Semaphore>,
    tx: mpsc::Sender<AgentEvent>,
    timeout_ms: u64,
) -> ToolResult {
    // Record the enqueue time so we can measure semaphore wait.
    // This covers any time spent blocked on the concurrency cap.
    let enqueue_time = Instant::now();

    // Acquire a concurrency permit before starting.
    // `wait_ms` below captures how long this took.
    let _permit = match sem.acquire_owned().await {
        Ok(permit) => permit,
        Err(_) => {
            // The semaphore was dropped (agent loop was shut down).
            // Return a graceful failure instead of panicking inside spawn.
            return ToolResult {
                tool_call_id: tc.id.clone(),
                content: "Tool execution aborted: concurrency gate closed".into(),
                success: false,
                wait_ms: elapsed_ms(enqueue_time),
                execute_duration_ms: 0,
            };
        }
    };
    let wait_ms = elapsed_ms(enqueue_time);

    let exec_start = Instant::now();

    // Notify that execution is starting (after permit acquired —
    // we only claim "started" once we have a slot, not when queued).
    let _ = tx
        .send(AgentEvent::ToolCallStart {
            tool_call: tc.clone(),
        })
        .await;

    let result =
        tokio::time::timeout(Duration::from_millis(timeout_ms), executor.execute(&tc)).await;
    let duration_ms = elapsed_ms(exec_start);

    let tool_result = match result {
        Ok(Ok(mut r)) => {
            // Stamp timing metrics onto the adapter result.
            // The MCP adapter (and any other ToolExecutorPort impl) cannot
            // measure concurrency wait time; we fill it in here where both
            // metrics are available.  We preserve the adapter's own
            // `execute_duration_ms` (pure execution time as it measured it).
            r.wait_ms = wait_ms;
            r
        }
        Ok(Err(e)) => ToolResult {
            tool_call_id: tc.id.clone(),
            content: format!("Tool execution error: {e}"),
            success: false,
            wait_ms,
            execute_duration_ms: duration_ms,
        },
        Err(_) => ToolResult {
            tool_call_id: tc.id.clone(),
            content: format!("Tool '{}' timed out after {timeout_ms} ms", tc.name),
            success: false,
            wait_ms,
            execute_duration_ms: duration_ms,
        },
    };

    // Notify that execution is complete.
    let _ = tx
        .send(AgentEvent::ToolCallComplete {
            result: tool_result.clone(),
        })
        .await;

    tool_result
}

// =============================================================================
// Public API
// =============================================================================

/// Execute all `calls` in parallel, emitting progress events on `tx`.
///
/// Returns one [`ToolResult`] per call in the same order as `calls`.
/// Results for timed-out or errored calls have `success: false`.
pub(crate) async fn execute_tools_parallel(
    calls: &[ToolCall],
    executor: &Arc<dyn ToolExecutorPort>,
    config: &AgentConfig,
    tx: &mpsc::Sender<AgentEvent>,
) -> Vec<ToolResult> {
    let semaphore = Arc::new(Semaphore::new(config.max_parallel_tools));
    let timeout_ms = config.tool_timeout_ms;

    // Spawn each tool call into a JoinSet rather than as detached tasks.
    // When this future is dropped (e.g. because AgentTaskGuard aborts the
    // parent agent task on client disconnect), the JoinSet is dropped and
    // every in-flight tool task is cancelled automatically — no resource leak.
    let mut set: JoinSet<(usize, ToolResult)> = JoinSet::new();

    for (i, tc) in calls.iter().enumerate() {
        let tc = tc.clone();
        let sem = Arc::clone(&semaphore);
        let executor = Arc::clone(executor);
        let tx = tx.clone();
        set.spawn(async move {
            let result = execute_single_tool(tc, executor, sem, tx, timeout_ms).await;
            (i, result)
        });
    }

    // Collect results.  Pre-fill with None so that any panicked slot (which
    // carries no index) can be identified and replaced with a failure result.
    let mut results: Vec<Option<ToolResult>> = vec![None; calls.len()];
    while let Some(join_result) = set.join_next().await {
        match join_result {
            Ok((i, result)) => results[i] = Some(result),
            Err(e) => warn!("Tool task panicked: {e}"),
        }
    }

    results
        .into_iter()
        .enumerate()
        .map(|(i, opt)| {
            opt.unwrap_or_else(|| ToolResult {
                tool_call_id: calls[i].id.clone(),
                content: "Tool task panicked".into(),
                success: false,
                wait_ms: 0,
                execute_duration_ms: 0,
            })
        })
        .collect()
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
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
        ToolCall { id: id.into(), name: name.into(), arguments: json!({}) }
    }

    // ---- Minimal test executors ---------------------------------------------

    struct OkExecutor;

    #[async_trait]
    impl ToolExecutorPort for OkExecutor {
        async fn list_tools(&self) -> Vec<ToolDefinition> { vec![] }
        async fn execute(&self, tc: &ToolCall) -> Result<ToolResult> {
            Ok(ToolResult {
                tool_call_id: tc.id.clone(),
                content: "ok".into(),
                success: true,
                wait_ms: 0,
                execute_duration_ms: 0,
            })
        }
    }

    struct SlowExecutor { millis: u64 }

    #[async_trait]
    impl ToolExecutorPort for SlowExecutor {
        async fn list_tools(&self) -> Vec<ToolDefinition> { vec![] }
        async fn execute(&self, tc: &ToolCall) -> Result<ToolResult> {
            tokio::time::sleep(std::time::Duration::from_millis(self.millis)).await;
            Ok(ToolResult {
                tool_call_id: tc.id.clone(),
                content: "slow ok".into(),
                success: true,
                wait_ms: 0,
                execute_duration_ms: self.millis,
            })
        }
    }

    // ---- Tests --------------------------------------------------------------

    #[tokio::test]
    async fn all_tools_return_results_in_order() {
        let (tx, mut rx) = mpsc::channel(32);
        let executor: Arc<dyn ToolExecutorPort> = Arc::new(OkExecutor);
        let calls: Vec<ToolCall> = (0..3).map(|i| call(&format!("c{i}"), "t")).collect();

        let results = execute_tools_parallel(&calls, &executor, &AgentConfig::default(), &tx).await;

        assert_eq!(results.len(), 3);
        for (i, r) in results.iter().enumerate() {
            assert_eq!(r.tool_call_id, format!("c{i}"));
            assert!(r.success);
        }
        // Each call emits ToolCallStart + ToolCallComplete (6 total).
        let mut event_count = 0;
        while rx.try_recv().is_ok() { event_count += 1; }
        assert_eq!(event_count, 6);
    }

    #[tokio::test]
    async fn timeout_produces_failure_result() {
        let (tx, _rx) = mpsc::channel(32);
        let executor: Arc<dyn ToolExecutorPort> = Arc::new(SlowExecutor { millis: 1_000 });
        let calls = vec![call("slow", "slow_tool")];
        let mut config = AgentConfig::default();
        config.tool_timeout_ms = 10;

        let results = execute_tools_parallel(&calls, &executor, &config, &tx).await;

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
            async fn list_tools(&self) -> Vec<ToolDefinition> { vec![] }
            async fn execute(&self, tc: &ToolCall) -> Result<ToolResult> {
                let prev = self.current.fetch_add(1, Ordering::SeqCst);
                let running = prev + 1;
                let mut peak = self.peak.load(Ordering::SeqCst);
                while running > peak {
                    match self.peak.compare_exchange(peak, running, Ordering::SeqCst, Ordering::SeqCst) {
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
                    wait_ms: 0,
                    execute_duration_ms: 20,
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

        execute_tools_parallel(&calls, &tracker, &config, &tx).await;

        let observed_peak = peak.load(Ordering::SeqCst);
        assert!(
            observed_peak <= 3,
            "peak concurrency {observed_peak} exceeded limit of 3"
        );
    }
}
