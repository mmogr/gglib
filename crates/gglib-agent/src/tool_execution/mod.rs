//! Parallel tool execution with bounded concurrency and per-tool timeout.
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

#[cfg(test)]
mod tests;

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
    // Shared failure constructor — avoids repeating the struct literal with the
    // same `tool_call_id` and `success: false` across every error branch.
    let error_result = |content: String| ToolResult {
        tool_call_id: tc.id.clone(),
        content,
        success: false,
    };

    // Record the enqueue time so we can measure semaphore wait.
    // This covers any time spent blocked on the concurrency cap.
    let enqueue_time = Instant::now();

    // Acquire a concurrency permit before starting.
    // `wait_ms` below captures how long this took.
    let Ok(_permit) = sem.acquire_owned().await else {
        // `acquire_owned` returns `Err` only when `Semaphore::close()` has
        // been called.  We never call `close()`, so this branch is
        // unreachable in normal operation.  It exists as a safety net in
        // case future refactors introduce explicit shutdown.
        return error_result("Tool execution aborted: concurrency gate closed".into());
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
        Ok(Ok(r)) => r,
        Ok(Err(e)) => error_result(format!("Tool execution error: {e}")),
        Err(_) => error_result(format!(
            "Tool '{}' timed out after {timeout_ms} ms",
            tc.name
        )),
    };

    // Notify that execution is complete.
    let _ = tx
        .send(AgentEvent::ToolCallComplete {
            result: tool_result.clone(),
            wait_ms,
            execute_duration_ms: duration_ms,
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
pub async fn execute_tools_parallel(
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
            })
        })
        .collect()
}
