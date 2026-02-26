/**
 * Unit tests for toolBatchExecution utilities.
 *
 * Covers:
 * - createConcurrencyLimiter: cap enforcement, sequential queuing, failure tolerance
 * - withToolTimeout: resolves before timeout, rejects after timeout
 * - executeToolBatch: index preservation, concurrency limit, failure isolation,
 *   onToolEvent lifecycle ordering, degenerate single-tool case
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import {
  createConcurrencyLimiter,
  withToolTimeout,
  executeToolBatch,
} from '../../../../src/hooks/useGglibRuntime/toolBatchExecution';
import type { AccumulatedToolCall } from '../../../../src/hooks/useGglibRuntime/accumulateToolCalls';
import type { ToolExecutionEvent } from '../../../../src/types/events/toolExecution';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeToolCall(id: string, name = 'tool'): AccumulatedToolCall {
  return {
    id,
    type: 'function',
    function: { name, arguments: '{}' },
  };
}

function makeSuccessResult(data: unknown = { ok: true }) {
  return { success: true as const, data };
}

function makeErrorResult(error = 'boom') {
  return { success: false as const, error };
}

// ---------------------------------------------------------------------------
// createConcurrencyLimiter
// ---------------------------------------------------------------------------

describe('createConcurrencyLimiter', () => {
  it('runs tasks immediately when under the concurrency cap', async () => {
    const limit = createConcurrencyLimiter(3);
    const order: number[] = [];

    await Promise.all([
      limit(() => Promise.resolve().then(() => { order.push(1); })),
      limit(() => Promise.resolve().then(() => { order.push(2); })),
      limit(() => Promise.resolve().then(() => { order.push(3); })),
    ]);

    expect(order).toEqual([1, 2, 3]);
  });

  it('queues tasks beyond the concurrency cap', async () => {
    const limit = createConcurrencyLimiter(2);
    let active = 0;
    let maxConcurrent = 0;

    const makeTask = () =>
      limit(async () => {
        active++;
        maxConcurrent = Math.max(maxConcurrent, active);
        // yield to allow other microtasks to run
        await new Promise(r => setTimeout(r, 0));
        active--;
      });

    await Promise.all([makeTask(), makeTask(), makeTask(), makeTask()]);

    // At most 2 should have been running simultaneously
    expect(maxConcurrent).toBeLessThanOrEqual(2);
  });

  it('decrements the active count on rejection so the queue is not permanently stalled', async () => {
    const limit = createConcurrencyLimiter(1);
    const results: Array<'ok' | 'err'> = [];

    await Promise.allSettled([
      limit(() => Promise.reject(new Error('fail'))).catch(() => { results.push('err'); }),
      limit(() => Promise.resolve()).then(() => { results.push('ok'); }),
    ]);

    expect(results).toEqual(['err', 'ok']);
  });

  it('passes the resolved value through unchanged', async () => {
    const limit = createConcurrencyLimiter(2);
    const result = await limit(() => Promise.resolve(42));
    expect(result).toBe(42);
  });
});

// ---------------------------------------------------------------------------
// withToolTimeout
// ---------------------------------------------------------------------------

describe('withToolTimeout', () => {
  it('resolves with the function result when it finishes before the timeout', async () => {
    const result = await withToolTimeout(() => Promise.resolve('done'), 100);
    expect(result).toBe('done');
  });

  it('rejects with a timeout error when the function takes too long', async () => {
    const never = () => new Promise<never>(() => {/* never resolves */});
    await expect(withToolTimeout(never, 10)).rejects.toThrow('timed out');
  });

  it('clears the timeout handle when the function resolves (no leaked timer)', async () => {
    const clearSpy = vi.spyOn(globalThis, 'clearTimeout');
    await withToolTimeout(() => Promise.resolve(), 1000);
    expect(clearSpy).toHaveBeenCalled();
    clearSpy.mockRestore();
  });

  it('clears the timeout handle when the function rejects (no leaked timer)', async () => {
    const clearSpy = vi.spyOn(globalThis, 'clearTimeout');
    await withToolTimeout(() => Promise.reject(new Error('fail')), 1000).catch(() => {});
    expect(clearSpy).toHaveBeenCalled();
    clearSpy.mockRestore();
  });
});

// ---------------------------------------------------------------------------
// executeToolBatch
// ---------------------------------------------------------------------------

// Mock the modules that executeToolBatch imports
vi.mock('../../../../src/services/platform', () => ({
  appLogger: {
    warn: vi.fn(),
    debug: vi.fn(),
    info: vi.fn(),
    error: vi.fn(),
  },
}));

vi.mock('../../../../src/hooks/useGglibRuntime/agentLoop', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../../../../src/hooks/useGglibRuntime/agentLoop')>();
  return {
    ...actual,
    MAX_PARALLEL_TOOLS: 3,
    TOOL_TIMEOUT_MS: 5000,
    withRetry: (fn: () => Promise<unknown>) => fn(),
  };
});

// The registry mock is set per-test below
const mockExecuteRawCall = vi.fn();
vi.mock('../../../../src/services/tools', () => ({
  getToolRegistry: () => ({ executeRawCall: mockExecuteRawCall }),
}));

describe('executeToolBatch', () => {
  beforeEach(() => {
    mockExecuteRawCall.mockReset();
  });

  // ── Degenerate / happy path ─────────────────────────────────────────────

  it('returns an empty array for zero tool calls', async () => {
    const results = await executeToolBatch([], vi.fn());
    expect(results).toEqual([]);
  });

  it('degenerate single-tool: resolves and returns the result', async () => {
    const data = { answer: 42 };
    mockExecuteRawCall.mockResolvedValueOnce(makeSuccessResult(data));

    const results = await executeToolBatch([makeToolCall('t1')], vi.fn());

    expect(results).toHaveLength(1);
    expect(results[0]).toEqual(makeSuccessResult(data));
  });

  // ── Result ordering ─────────────────────────────────────────────────────

  it('preserves original index order regardless of completion order', async () => {
    // Tools resolve in reverse order (slowest first declared = index 0)
    let resolveLast!: (v: unknown) => void;
    const slow = new Promise(r => { resolveLast = r; });

    mockExecuteRawCall
      .mockReturnValueOnce(slow.then(() => makeSuccessResult({ id: 0 })))
      .mockResolvedValueOnce(makeSuccessResult({ id: 1 }))
      .mockResolvedValueOnce(makeSuccessResult({ id: 2 }));

    const calls = [makeToolCall('t0'), makeToolCall('t1'), makeToolCall('t2')];
    const batchPromise = executeToolBatch(calls, vi.fn());

    // Let tools 1 and 2 settle first
    await Promise.resolve();
    resolveLast(undefined);

    const results = await batchPromise;
    expect(results[0]).toEqual(makeSuccessResult({ id: 0 }));
    expect(results[1]).toEqual(makeSuccessResult({ id: 1 }));
    expect(results[2]).toEqual(makeSuccessResult({ id: 2 }));
  });

  // ── Failure isolation ───────────────────────────────────────────────────

  it('does not cancel other tools when one tool fails', async () => {
    mockExecuteRawCall
      .mockRejectedValueOnce(new Error('tool A exploded'))
      .mockResolvedValueOnce(makeSuccessResult({ b: true }));

    const results = await executeToolBatch(
      [makeToolCall('tA'), makeToolCall('tB')],
      vi.fn(),
    );

    expect(results[0]).toEqual({ success: false, error: expect.stringContaining('tool A exploded') });
    expect(results[1]).toEqual(makeSuccessResult({ b: true }));
  });

  it('synthesises a failure ToolResult when a tool rejects', async () => {
    mockExecuteRawCall.mockRejectedValueOnce(new Error('network gone'));

    const results = await executeToolBatch([makeToolCall('t1')], vi.fn());

    expect(results[0].success).toBe(false);
    expect((results[0] as { success: false; error: string }).error).toContain('network gone');
  });

  // ── onToolEvent lifecycle ───────────────────────────────────────────────

  it('emits tool-start before tool-complete for a successful tool', async () => {
    mockExecuteRawCall.mockResolvedValueOnce(makeSuccessResult());

    const events: ToolExecutionEvent[] = [];
    await executeToolBatch([makeToolCall('t1', 'search')], e => events.push(e));

    expect(events[0].type).toBe('tool-start');
    expect(events[0].toolName).toBe('search');
    expect(events[0].toolCallId).toBe('t1');

    expect(events[1].type).toBe('tool-complete');
    expect(events[1].toolCallId).toBe('t1');
  });

  it('emits tool-start before tool-error for a failing tool', async () => {
    mockExecuteRawCall.mockRejectedValueOnce(new Error('oops'));

    const events: ToolExecutionEvent[] = [];
    await executeToolBatch([makeToolCall('t1', 'write')], e => events.push(e));

    expect(events[0].type).toBe('tool-start');
    expect(events[1].type).toBe('tool-error');
    expect(events[1].toolCallId).toBe('t1');
  });

  it('includes durationMs on settled events', async () => {
    mockExecuteRawCall.mockResolvedValueOnce(makeSuccessResult());

    const events: ToolExecutionEvent[] = [];
    await executeToolBatch([makeToolCall('t1')], e => events.push(e));

    const complete = events.find(e => e.type === 'tool-complete');
    expect(complete).toBeDefined();
    expect((complete as { durationMs: number }).durationMs).toBeGreaterThanOrEqual(0);
  });

  it('emits data (not a JSON string) on tool-complete events', async () => {
    const payload = { nested: { value: 99 } };
    mockExecuteRawCall.mockResolvedValueOnce(makeSuccessResult(payload));

    const events: ToolExecutionEvent[] = [];
    await executeToolBatch([makeToolCall('t1')], e => events.push(e));

    const complete = events.find(e => e.type === 'tool-complete') as Extract<
      ToolExecutionEvent,
      { type: 'tool-complete' }
    >;
    expect(complete).toBeDefined();
    // data must be the raw object, not a JSON string
    expect(typeof complete.data).not.toBe('string');
    expect(complete.data).toEqual(payload);
  });

  it('emits events for all tools in a batch, even when some fail', async () => {
    mockExecuteRawCall
      .mockRejectedValueOnce(new Error('a failed'))
      .mockResolvedValueOnce(makeSuccessResult())
      .mockRejectedValueOnce(new Error('c failed'));

    const events: ToolExecutionEvent[] = [];
    await executeToolBatch(
      [makeToolCall('tA'), makeToolCall('tB'), makeToolCall('tC')],
      e => events.push(e),
    );

    const starts  = events.filter(e => e.type === 'tool-start');
    const settled = events.filter(e => e.type !== 'tool-start');
    expect(starts).toHaveLength(3);
    expect(settled).toHaveLength(3);
  });

  it('continues the batch when the onToolEvent callback throws', async () => {
    mockExecuteRawCall.mockResolvedValue(makeSuccessResult());

    let callCount = 0;
    const throwingCb = () => {
      callCount++;
      if (callCount === 1) throw new Error('cb crash');
    };

    // Should not throw
    const results = await executeToolBatch(
      [makeToolCall('t1'), makeToolCall('t2')],
      throwingCb,
    );

    expect(results).toHaveLength(2);
  });
});
