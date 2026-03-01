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

