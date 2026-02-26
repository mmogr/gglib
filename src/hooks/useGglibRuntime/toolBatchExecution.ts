/**
 * Parallel tool batch execution utilities.
 *
 * Houses the concurrency limiter, per-tool timeout wrapper, and the batch
 * runner. Intentionally decoupled from React — callers supply an
 * onToolSettled callback to handle UI updates and digest accumulation as each
 * tool completes, without waiting for the full batch to finish.
 *
 * @module toolBatchExecution
 */

import { appLogger } from '../../services/platform';
import type { AccumulatedToolCall } from './accumulateToolCalls';
import type { ToolResult } from '../../services/tools';
import { getToolRegistry } from '../../services/tools';
import { withRetry, MAX_PARALLEL_TOOLS, TOOL_TIMEOUT_MS } from './agentLoop';

// =============================================================================
// Concurrency limiter
// =============================================================================

/**
 * Create a simple concurrency semaphore.
 *
 * Ensures at most `concurrency` wrapped promises run simultaneously.
 * A `finally` block decrements the active count so a rejected promise never
 * permanently stalls the queue.
 */
export function createConcurrencyLimiter(concurrency: number) {
  let active = 0;
  const queue: (() => void)[] = [];

  return function limit<T>(fn: () => Promise<T>): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      const run = () => {
        active++;
        fn()
          .then(resolve, reject)
          .finally(() => {
            active--;
            queue.shift()?.();
          });
      };

      if (active < concurrency) {
        run();
      } else {
        queue.push(run);
      }
    });
  };
}

// =============================================================================
// Per-tool timeout wrapper
// =============================================================================

/**
 * Race a promise-returning function against a timeout.
 *
 * Clears the timeout handle in a `finally` block to prevent a leaked
 * `setTimeout` accumulating across a long agentic session with many tool calls.
 *
 * NOTE: `Promise.race` leaves the underlying `fn()` executing as a detached
 * background task if the timeout fires first. Consider piping an AbortSignal
 * into `executeRawCall` in a future pass to enable real cancellation.
 */
export function withToolTimeout<T>(fn: () => Promise<T>, ms: number): Promise<T> {
  let handle: ReturnType<typeof setTimeout> | undefined;
  const timeoutPromise = new Promise<never>((_, reject) => {
    handle = setTimeout(
      () => reject(new Error(`Tool timed out after ${ms}ms`)),
      ms,
    );
  });
  return Promise.race([fn(), timeoutPromise]).finally(() => {
    clearTimeout(handle);
  });
}

// =============================================================================
// Batch executor
// =============================================================================

/**
 * Callback invoked immediately as each individual tool settles.
 *
 * @param index    The original position in the toolCalls array.
 * @param toolCall The tool call that settled.
 * @param result   Normalised ToolResult (success or failure).
 */
export type OnToolSettled = (
  index: number,
  toolCall: AccumulatedToolCall,
  result: ToolResult,
) => void;

/**
 * Execute all tool calls concurrently with a concurrency cap and per-tool
 * timeout, streaming results to the caller as each tool finishes.
 *
 * Guarantees:
 * - `onToolSettled` is called as soon as each individual tool settles, so the
 *   UI can reflect completed tools without waiting for the whole batch.
 * - The original array `index` is preserved in `onToolSettled`, regardless of
 *   which promise resolves first, so order-sensitive data structures stay
 *   consistent.
 * - Synchronous errors thrown by `onToolSettled` are caught and logged so a
 *   UI rendering failure cannot crash the rest of the batch.
 * - Returns `ToolResult[]` in the same order as the input array, ready for
 *   inclusion in the API message history.
 */
export async function executeToolBatch(
  toolCalls: AccumulatedToolCall[],
  onToolSettled: OnToolSettled,
): Promise<ToolResult[]> {
  const limit = createConcurrencyLimiter(MAX_PARALLEL_TOOLS);
  const results: ToolResult[] = new Array(toolCalls.length);

  const notifySettled = (
    index: number,
    toolCall: AccumulatedToolCall,
    result: ToolResult,
  ): void => {
    results[index] = result;
    try {
      onToolSettled(index, toolCall, result);
    } catch (cbErr) {
      appLogger.warn('hook.runtime', 'onToolSettled callback threw', {
        index,
        toolName: toolCall.function.name,
        error: String(cbErr),
      });
    }
  };

  await Promise.allSettled(
    toolCalls.map((toolCall, index) =>
      limit(() =>
        withToolTimeout(
          () =>
            withRetry(
              () =>
                getToolRegistry().executeRawCall({
                  id: toolCall.id,
                  type: 'function',
                  function: toolCall.function,
                }),
              { maxRetries: 2, baseDelayMs: 250 },
            ),
          TOOL_TIMEOUT_MS,
        ),
      ).then(
        (result) => {
          notifySettled(index, toolCall, result);
        },
        (err: unknown) => {
          const error = `[${toolCall.function.name}] ${String(
            (err as { message?: string })?.message ?? err ?? 'Unknown error',
          )}`;
          notifySettled(index, toolCall, { success: false, error });
        },
      ),
    ),
  );

  return results;
}
