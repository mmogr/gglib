/**
 * Tests for accumulateToolCalls utility.
 *
 * These tests verify the tool call accumulator:
 * - Initializes new tool calls from first delta
 * - Merges subsequent deltas (appending arguments)
 * - Handles parallel tool calls by index
 * - Returns sorted array from getState()
 * - Resets state correctly
 */

import { describe, it, expect } from 'vitest';
import {
  createToolCallAccumulator,
  type AccumulatedToolCall,
} from '../../../../src/hooks/useGglibRuntime/accumulateToolCalls';
import type { ToolCallDelta } from '../../../../src/hooks/useGglibRuntime/parseSSEStream';

// =============================================================================
// Tests
// =============================================================================

describe('createToolCallAccumulator', () => {
  describe('initialization', () => {
    it('starts with empty state', () => {
      const acc = createToolCallAccumulator();
      const state = acc.getState();

      expect(state.toolCalls).toHaveLength(0);
      expect(state.hasToolCalls).toBe(false);
    });
  });

  describe('single tool call', () => {
    it('initializes from first delta with all fields', () => {
      const acc = createToolCallAccumulator();

      acc.push({
        index: 0,
        id: 'call_123',
        type: 'function',
        function: {
          name: 'get_weather',
          arguments: '{"city":',
        },
      });

      const state = acc.getState();

      expect(state.hasToolCalls).toBe(true);
      expect(state.toolCalls).toHaveLength(1);
      expect(state.toolCalls[0]).toEqual({
        id: 'call_123',
        type: 'function',
        function: {
          name: 'get_weather',
          arguments: '{"city":',
        },
      });
    });

    it('appends arguments from subsequent deltas', () => {
      const acc = createToolCallAccumulator();

      acc.push({
        index: 0,
        id: 'call_123',
        type: 'function',
        function: {
          name: 'get_weather',
          arguments: '{"city":',
        },
      });

      acc.push({
        index: 0,
        function: {
          arguments: '"NYC"',
        },
      });

      acc.push({
        index: 0,
        function: {
          arguments: '}',
        },
      });

      const state = acc.getState();

      expect(state.toolCalls[0].function.arguments).toBe('{"city":"NYC"}');
    });

    it('handles delta with only id', () => {
      const acc = createToolCallAccumulator();

      acc.push({ index: 0 });
      acc.push({ index: 0, id: 'call_late' });

      const state = acc.getState();

      expect(state.toolCalls[0].id).toBe('call_late');
    });

    it('handles delta with only function name', () => {
      const acc = createToolCallAccumulator();

      acc.push({ index: 0, id: 'call_1' });
      acc.push({ index: 0, function: { name: 'late_name' } });

      const state = acc.getState();

      expect(state.toolCalls[0].function.name).toBe('late_name');
    });
  });

  describe('parallel tool calls', () => {
    it('accumulates multiple tool calls by index', () => {
      const acc = createToolCallAccumulator();

      acc.push({
        index: 0,
        id: 'call_a',
        type: 'function',
        function: { name: 'tool_a', arguments: '{}' },
      });

      acc.push({
        index: 1,
        id: 'call_b',
        type: 'function',
        function: { name: 'tool_b', arguments: '{}' },
      });

      const state = acc.getState();

      expect(state.toolCalls).toHaveLength(2);
      expect(state.toolCalls[0].function.name).toBe('tool_a');
      expect(state.toolCalls[1].function.name).toBe('tool_b');
    });

    it('merges deltas for correct index', () => {
      const acc = createToolCallAccumulator();

      // First deltas for both
      acc.push({
        index: 0,
        id: 'call_a',
        function: { name: 'tool_a', arguments: '{"x":' },
      });
      acc.push({
        index: 1,
        id: 'call_b',
        function: { name: 'tool_b', arguments: '{"y":' },
      });

      // Argument continuations
      acc.push({ index: 0, function: { arguments: '1}' } });
      acc.push({ index: 1, function: { arguments: '2}' } });

      const state = acc.getState();

      expect(state.toolCalls[0].function.arguments).toBe('{"x":1}');
      expect(state.toolCalls[1].function.arguments).toBe('{"y":2}');
    });

    it('returns tool calls sorted by index', () => {
      const acc = createToolCallAccumulator();

      // Push in reverse order
      acc.push({
        index: 2,
        id: 'call_c',
        function: { name: 'tool_c' },
      });
      acc.push({
        index: 0,
        id: 'call_a',
        function: { name: 'tool_a' },
      });
      acc.push({
        index: 1,
        id: 'call_b',
        function: { name: 'tool_b' },
      });

      const state = acc.getState();

      expect(state.toolCalls.map((tc) => tc.function.name)).toEqual([
        'tool_a',
        'tool_b',
        'tool_c',
      ]);
    });
  });

  describe('reset', () => {
    it('clears all accumulated state', () => {
      const acc = createToolCallAccumulator();

      acc.push({
        index: 0,
        id: 'call_1',
        function: { name: 'tool', arguments: '{}' },
      });

      expect(acc.getState().hasToolCalls).toBe(true);

      acc.reset();

      const state = acc.getState();
      expect(state.toolCalls).toHaveLength(0);
      expect(state.hasToolCalls).toBe(false);
    });

    it('allows reuse after reset', () => {
      const acc = createToolCallAccumulator();

      acc.push({ index: 0, id: 'old', function: { name: 'old_tool' } });
      acc.reset();
      acc.push({ index: 0, id: 'new', function: { name: 'new_tool' } });

      const state = acc.getState();

      expect(state.toolCalls).toHaveLength(1);
      expect(state.toolCalls[0].id).toBe('new');
      expect(state.toolCalls[0].function.name).toBe('new_tool');
    });
  });

  describe('edge cases', () => {
    it('handles delta with empty function object', () => {
      const acc = createToolCallAccumulator();

      acc.push({ index: 0, id: 'call_1', function: {} });

      const state = acc.getState();

      expect(state.toolCalls[0].function.name).toBe('');
      expect(state.toolCalls[0].function.arguments).toBe('');
    });

    it('handles delta with no function', () => {
      const acc = createToolCallAccumulator();

      acc.push({ index: 0, id: 'call_1' });

      const state = acc.getState();

      expect(state.toolCalls[0].function.name).toBe('');
      expect(state.toolCalls[0].function.arguments).toBe('');
    });

    it('uses default type of "function"', () => {
      const acc = createToolCallAccumulator();

      acc.push({ index: 0, id: 'call_1' });

      const state = acc.getState();

      expect(state.toolCalls[0].type).toBe('function');
    });

    it('does not duplicate arguments on update without arguments', () => {
      const acc = createToolCallAccumulator();

      acc.push({
        index: 0,
        id: 'call_1',
        function: { name: 'tool', arguments: '{"a":1}' },
      });

      // Delta without arguments
      acc.push({ index: 0, function: { name: 'renamed_tool' } });

      const state = acc.getState();

      expect(state.toolCalls[0].function.arguments).toBe('{"a":1}');
      expect(state.toolCalls[0].function.name).toBe('renamed_tool');
    });
  });
});
