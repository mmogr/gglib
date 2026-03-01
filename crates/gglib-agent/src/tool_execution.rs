//! Parallel tool execution with bounded concurrency and per-tool timeout.
//!
//! This module is the Rust port of `executeToolBatch` from
//! `src/hooks/useGglibRuntime/toolBatchExecution.ts`.
//!
//! # Behaviour
//!
//! - All tool calls in a batch are dispatched concurrently.
//! - An [`tokio::sync::Semaphore`] caps the number of *simultaneously running*
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

use futures_util::future::join_all;
use gglib_core::ports::ToolExecutorPort;
use gglib_core::{AgentConfig, AgentEvent, ToolCall, ToolResult};
use tokio::sync::{Semaphore, mpsc};

use gglib_core::elapsed_ms;

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

    let handles: Vec<_> = calls
        .iter()
        .map(|tc| {
            let tc = tc.clone();
            let sem = Arc::clone(&semaphore);
            let executor = Arc::clone(executor);
            let tx = tx.clone();
            tokio::spawn(async move {
                // Record the enqueue time so we can measure semaphore wait.
                // This covers any time spent blocked on the concurrency cap.
                let enqueue_time = Instant::now();

                // Acquire a concurrency permit before starting.
                // `wait_ms` below captures how long this took.
                let _permit = sem.acquire_owned().await.expect("semaphore closed");
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
                    tokio::time::timeout(Duration::from_millis(timeout_ms), executor.execute(&tc))
                        .await;
                let duration_ms = elapsed_ms(exec_start);

                let tool_result = match result {
                    Ok(Ok(mut r)) => {
                        // Stamp timing metrics onto the adapter result.
                        // The MCP adapter (and any other ToolExecutorPort impl)
                        // cannot measure concurrency wait time; we fill it in
                        // here where both metrics are available.  The adapter's
                        // own duration_ms is also overridden so the total
                        // wall-clock time (from permit acquisition to finish)
                        // is consistently reported.
                        r.wait_ms = wait_ms;
                        r.duration_ms = duration_ms;
                        r
                    }
                    Ok(Err(e)) => ToolResult {
                        tool_call_id: tc.id.clone(),
                        content: format!("Tool execution error: {e}"),
                        success: false,
                        wait_ms,
                        duration_ms,
                    },
                    Err(_) => ToolResult {
                        tool_call_id: tc.id.clone(),
                        content: format!("Tool '{}' timed out after {timeout_ms} ms", tc.name),
                        success: false,
                        wait_ms,
                        duration_ms,
                    },
                };

                // Notify that execution is complete.
                let _ = tx
                    .send(AgentEvent::ToolCallComplete {
                        result: tool_result.clone(),
                    })
                    .await;

                tool_result
            })
        })
        .collect();

    // Await all handles in order.  JoinError (task panic) is treated as failure.
    join_all(handles)
        .await
        .into_iter()
        .enumerate()
        .map(|(i, join_result)| {
            join_result.unwrap_or_else(|e| ToolResult {
                tool_call_id: calls[i].id.clone(),
                content: format!("Tool task panicked: {e}"),
                success: false,
                wait_ms: 0,
                duration_ms: 0,
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
                duration_ms: 0,
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
                duration_ms: self.millis,
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
        let config = AgentConfig { tool_timeout_ms: 10, ..Default::default() };

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
                    duration_ms: 20,
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
        let config = AgentConfig { max_parallel_tools: 3, ..Default::default() };

        execute_tools_parallel(&calls, &tracker, &config, &tx).await;

        let observed_peak = peak.load(Ordering::SeqCst);
        assert!(
            observed_peak <= 3,
            "peak concurrency {observed_peak} exceeded limit of 3"
        );
    }
}
