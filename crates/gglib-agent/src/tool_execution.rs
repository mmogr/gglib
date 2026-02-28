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

// =============================================================================
// Public API
// =============================================================================

/// Execute all `calls` in parallel, emitting progress events on `tx`.
///
/// Returns one [`ToolResult`] per call in the same order as `calls`.
/// Results for timed-out or errored calls have `success: false`.
pub async fn execute_tools_parallel(
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
                let wait_ms = u64::try_from(enqueue_time.elapsed().as_millis()).unwrap_or(u64::MAX);

                // Notify that execution is starting (after permit acquired —
                // we only claim "started" once we have a slot, not when queued).
                let _ = tx
                    .send(AgentEvent::ToolCallStart {
                        tool_call: tc.clone(),
                    })
                    .await;

                let exec_start = Instant::now();
                let result =
                    tokio::time::timeout(Duration::from_millis(timeout_ms), executor.execute(&tc))
                        .await;
                let duration_ms =
                    u64::try_from(exec_start.elapsed().as_millis()).unwrap_or(u64::MAX);

                let tool_result = match result {
                    Ok(Ok(r)) => r,
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

    use async_trait::async_trait;
    use gglib_core::ports::ToolExecutorPort;
    use gglib_core::{AgentConfig, ToolCall, ToolDefinition, ToolResult};
    use serde_json::json;
    use tokio::sync::mpsc;

    use super::*;

    // ---- Mock executor -------------------------------------------------------

    struct InstantExecutor;

    #[async_trait]
    impl ToolExecutorPort for InstantExecutor {
        async fn list_tools(&self) -> Vec<ToolDefinition> {
            vec![]
        }
        async fn execute(&self, call: &ToolCall) -> anyhow::Result<ToolResult> {
            Ok(ToolResult {
                tool_call_id: call.id.clone(),
                content: "ok".into(),
                success: true,
                wait_ms: 0,
                duration_ms: 0,
            })
        }
    }

    struct SlowExecutor {
        delay_ms: u64,
    }

    #[async_trait]
    impl ToolExecutorPort for SlowExecutor {
        async fn list_tools(&self) -> Vec<ToolDefinition> {
            vec![]
        }
        async fn execute(&self, call: &ToolCall) -> anyhow::Result<ToolResult> {
            tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
            Ok(ToolResult {
                tool_call_id: call.id.clone(),
                content: "slow ok".into(),
                success: true,
                wait_ms: 0,
                duration_ms: self.delay_ms,
            })
        }
    }

    // ---- Tests ---------------------------------------------------------------

    #[tokio::test]
    async fn all_tools_return_results_in_order() {
        let (tx, mut rx) = mpsc::channel(32);
        let calls: Vec<ToolCall> = (0..3)
            .map(|i| ToolCall {
                id: format!("c{i}"),
                name: "t".into(),
                arguments: json!({}),
            })
            .collect();

        let results = execute_tools_parallel(
            &calls,
            &(Arc::new(InstantExecutor) as Arc<dyn ToolExecutorPort>),
            &AgentConfig::default(),
            &tx,
        )
        .await;

        assert_eq!(results.len(), 3);
        for (i, r) in results.iter().enumerate() {
            assert_eq!(r.tool_call_id, format!("c{i}"));
            assert!(r.success);
        }
        // Each call emits start + complete events (6 total)
        let mut event_count = 0;
        while rx.try_recv().is_ok() {
            event_count += 1;
        }
        assert_eq!(event_count, 6);
    }

    #[tokio::test]
    async fn timeout_produces_failure_result() {
        let (tx, _rx) = mpsc::channel(32);
        let calls = vec![ToolCall {
            id: "slow".into(),
            name: "slow_tool".into(),
            arguments: json!({}),
        }];

        let config = AgentConfig {
            tool_timeout_ms: 10, // 10 ms timeout
            ..Default::default()
        };

        let results = execute_tools_parallel(
            &calls,
            &(Arc::new(SlowExecutor { delay_ms: 1_000 }) as Arc<dyn ToolExecutorPort>),
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
        use std::sync::atomic::{AtomicUsize, Ordering};

        // Track the peak concurrency during execution.
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
                // Update peak
                let mut peak = self.peak.load(Ordering::SeqCst);
                while running > peak {
                    match self.peak.compare_exchange(
                        peak,
                        running,
                        Ordering::SeqCst,
                        Ordering::SeqCst,
                    ) {
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

        let calls: Vec<ToolCall> = (0..10)
            .map(|i| ToolCall {
                id: format!("c{i}"),
                name: "t".into(),
                arguments: json!({}),
            })
            .collect();

        let (tx, _rx) = mpsc::channel(64);
        let config = AgentConfig {
            max_parallel_tools: 3,
            ..Default::default()
        };

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
}
