/**
 * Tool call accumulator for streaming OpenAI-compatible tool calls.
 *
 * Tool calls stream incrementally with partial JSON arguments across multiple
 * chunks. This module provides a stateful accumulator that merges deltas into
 * complete tool calls.
 *
 * @module accumulateToolCalls
 */

import type { ToolCallDelta } from './parseSSEStream';

// =============================================================================
// Types
// =============================================================================

/** Accumulated tool call (after all deltas combined) */
export interface AccumulatedToolCall {
  /** Unique ID for this tool call */
  id: string;
  /** Tool type - always "function" */
  type: string;
  /** Function call details */
  function: {
    /** Name of the function to call */
    name: string;
    /** JSON string of arguments */
    arguments: string;
  };
}

/** Immutable snapshot of accumulated tool calls */
export interface ToolCallAccumulatorState {
  /** Tool calls indexed by their stream index, sorted by index */
  toolCalls: AccumulatedToolCall[];
  /** Whether any tool calls have been received */
  hasToolCalls: boolean;
}

/** Tool call accumulator interface */
export interface ToolCallAccumulator {
  /**
   * Push a tool call delta to be merged into the accumulated state.
   * @param delta - The streaming tool call delta
   */
  push(delta: ToolCallDelta): void;

  /**
   * Get the current accumulated state as an immutable snapshot.
   * Tool calls are sorted by their stream index.
   */
  getState(): ToolCallAccumulatorState;

  /**
   * Reset the accumulator to its initial empty state.
   */
  reset(): void;
}

// =============================================================================
// Factory
// =============================================================================

/**
 * Create a new tool call accumulator.
 *
 * The accumulator merges streaming tool call deltas by their index, handling:
 * - Initial delta with id, type, and function name
 * - Subsequent deltas with partial argument strings to append
 *
 * @returns A new ToolCallAccumulator instance
 *
 * @example
 * ```ts
 * const accumulator = createToolCallAccumulator();
 *
 * for await (const delta of parseSSEStream(reader)) {
 *   if (delta.toolCalls) {
 *     for (const tc of delta.toolCalls) {
 *       accumulator.push(tc);
 *     }
 *   }
 * }
 *
 * const { toolCalls } = accumulator.getState();
 * console.log('Accumulated tool calls:', toolCalls);
 * ```
 */
export function createToolCallAccumulator(): ToolCallAccumulator {
  // Internal state: Map from stream index to accumulated tool call
  let state = new Map<number, AccumulatedToolCall>();

  function push(delta: ToolCallDelta): void {
    const existing = state.get(delta.index);

    if (!existing) {
      // First delta for this index - initialize
      state.set(delta.index, {
        id: delta.id ?? '',
        type: delta.type ?? 'function',
        function: {
          name: delta.function?.name ?? '',
          arguments: delta.function?.arguments ?? '',
        },
      });
    } else {
      // Merge with existing
      if (delta.id) existing.id = delta.id;
      if (delta.type) existing.type = delta.type;
      if (delta.function?.name) existing.function.name = delta.function.name;
      if (delta.function?.arguments) {
        existing.function.arguments += delta.function.arguments;
      }
    }
  }

  function getState(): ToolCallAccumulatorState {
    const toolCalls = Array.from(state.entries())
      .sort(([a], [b]) => a - b)
      .map(([, call]) => call);

    return {
      toolCalls,
      hasToolCalls: toolCalls.length > 0,
    };
  }

  function reset(): void {
    state = new Map();
  }

  return {
    push,
    getState,
    reset,
  };
}
