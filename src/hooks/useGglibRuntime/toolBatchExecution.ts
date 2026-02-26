/**
 * Parallel tool batch execution utilities.
 *
 * Houses the concurrency limiter, per-tool timeout wrapper, and the batch
 * runner. Intentionally decoupled from React — callers supply an
 * onToolEvent callback that receives lifecycle events ('tool-start',
 * 'tool-complete', 'tool-error') as each tool progresses, enabling live UI
 * updates and telemetry without waiting for the full batch to finish.
 *
 * @module toolBatchExecution
 */

import { appLogger } from '../../services/platform';
import type { AccumulatedToolCall } from './accumulateToolCalls';
import type { ToolResult } from '../../services/tools';
import { getToolRegistry } from '../../services/tools';
import { withRetry, MAX_PARALLEL_TOOLS, TOOL_TIMEOUT_MS } from './agentLoop';
import type { OnToolEvent } from '../../types/events/toolExecution';

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

// OnToolSettled is superseded by OnToolEvent (re-exported for any legacy callers).
export type { OnToolEvent } from '../../types/events/toolExecution';

/**
 * Execute all tool calls concurrently with a concurrency cap and per-tool
 * timeout, streaming results to the caller as each tool finishes.
 *
 * Guarantees:
 * - `onToolEvent` is called for each lifecycle stage ('tool-start',
 *   'tool-complete', 'tool-error') so callers can update UI and telemetry
 *   without waiting for the whole batch to finish.
 * - `performance.now()` is used for duration measurement — immune to system
 *   clock adjustments and higher-precision than `Date.now()`.
 * - The original array `index` is preserved in results, regardless of which
 *   promise resolves first, so order-sensitive data structures stay consistent.
 * - Synchronous errors thrown by `onToolEvent` are caught and logged so a
 *   UI rendering failure cannot crash the rest of the batch.
 * - Returns `ToolResult[]` in the same order as the input array, ready for
 *   inclusion in the API message history.
 */
export async function executeToolBatch(
  toolCalls: AccumulatedToolCall[],
  onToolEvent: OnToolEvent,
): Promise<ToolResult[]> {
  const limit = createConcurrencyLimiter(MAX_PARALLEL_TOOLS);
  const results: ToolResult[] = new Array(toolCalls.length);

  const safeEmit = (event: Parameters<OnToolEvent>[0]): void => {
    try {
      onToolEvent(event);
    } catch (cbErr) {
      appLogger.warn('hook.runtime', 'onToolEvent callback threw', {
        type: event.type,
        toolName: event.toolName,
        error: String(cbErr),
      });
    }
  };

  await Promise.allSettled(
    toolCalls.map((toolCall, index) =>
      limit(() => {
        // Record start time before invoking the tool; emit 'tool-start'.
        const startPerf = performance.now();
        safeEmit({
          type: 'tool-start',
          toolCallId: toolCall.id,
          toolName: toolCall.function.name,
          timestamp: Date.now(),
        });

        return withToolTimeout(
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
        ).then(
          (result) => {
            const durationMs = performance.now() - startPerf;
            results[index] = result;
            safeEmit({
              type: 'tool-complete',
              toolCallId: toolCall.id,
              toolName: toolCall.function.name,
              timestamp: Date.now(),
              data: result.success ? result.data : {},
              durationMs,
            });
          },
          (err: unknown) => {
            const durationMs = performance.now() - startPerf;
            const error = `[${toolCall.function.name}] ${String(
              (err as { message?: string })?.message ?? err ?? 'Unknown error',
            )}`;
            results[index] = { success: false, error };
            safeEmit({
              type: 'tool-error',
              toolCallId: toolCall.id,
              toolName: toolCall.function.name,
              timestamp: Date.now(),
              error,
              durationMs,
            });
          },
        );
      }),
    ),
  );

  return results;
}
