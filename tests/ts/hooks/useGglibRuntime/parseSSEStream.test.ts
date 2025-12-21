/**
 * Tests for parseSSEStream utility.
 *
 * These tests verify the SSE parsing logic handles:
 * - Well-formed single events
 * - Multiple events in a single chunk
 * - Events split across chunks
 * - Empty data lines and comments
 * - [DONE] stream termination
 * - Nested data: prefixes (backend + llama-server)
 */

import { describe, it, expect } from 'vitest';
import { parseSSEStream, type StreamDelta } from '../../../../src/hooks/useGglibRuntime/parseSSEStream';

// =============================================================================
// Test Helpers
// =============================================================================

/**
 * Create a ReadableStream from an array of string chunks.
 * Simulates how the browser receives SSE data in pieces.
 */
function makeStream(chunks: string[]): ReadableStream<Uint8Array> {
  const encoder = new TextEncoder();
  return new ReadableStream({
    start(controller) {
      for (const chunk of chunks) {
        controller.enqueue(encoder.encode(chunk));
      }
      controller.close();
    },
  });
}

/**
 * Collect all deltas from a parseSSEStream generator.
 */
async function collectDeltas(
  stream: ReadableStream<Uint8Array>,
  abortSignal?: AbortSignal
): Promise<StreamDelta[]> {
  const reader = stream.getReader();
  const deltas: StreamDelta[] = [];
  for await (const delta of parseSSEStream(reader, abortSignal)) {
    deltas.push(delta);
  }
  return deltas;
}

// =============================================================================
// Tests
// =============================================================================

describe('parseSSEStream', () => {
  describe('basic parsing', () => {
    it('parses a single content event', async () => {
      const stream = makeStream([
        'data: {"choices":[{"delta":{"content":"Hello"}}]}\n\n',
      ]);

      const deltas = await collectDeltas(stream);

      expect(deltas).toHaveLength(1);
      expect(deltas[0].content).toBe('Hello');
      expect(deltas[0].reasoningContent).toBeNull();
      expect(deltas[0].toolCalls).toBeNull();
      expect(deltas[0].finishReason).toBeNull();
    });

    it('parses multiple events in a single chunk', async () => {
      const stream = makeStream([
        'data: {"choices":[{"delta":{"content":"Hel"}}]}\n\n' +
          'data: {"choices":[{"delta":{"content":"lo"}}]}\n\n',
      ]);

      const deltas = await collectDeltas(stream);

      expect(deltas).toHaveLength(2);
      expect(deltas[0].content).toBe('Hel');
      expect(deltas[1].content).toBe('lo');
    });

    it('parses events split across chunks', async () => {
      const stream = makeStream([
        'data: {"choices":[{"delta":{"con',
        'tent":"Hello"}}]}\n\n',
      ]);

      const deltas = await collectDeltas(stream);

      expect(deltas).toHaveLength(1);
      expect(deltas[0].content).toBe('Hello');
    });

    it('handles [DONE] termination', async () => {
      const stream = makeStream([
        'data: {"choices":[{"delta":{"content":"Hi"}}]}\n\n',
        'data: [DONE]\n\n',
        'data: {"choices":[{"delta":{"content":"Ignored"}}]}\n\n',
      ]);

      const deltas = await collectDeltas(stream);

      expect(deltas).toHaveLength(1);
      expect(deltas[0].content).toBe('Hi');
    });
  });

  describe('finish_reason handling', () => {
    it('extracts finish_reason from final chunk', async () => {
      const stream = makeStream([
        'data: {"choices":[{"delta":{"content":"Done"},"finish_reason":"stop"}]}\n\n',
      ]);

      const deltas = await collectDeltas(stream);

      expect(deltas).toHaveLength(1);
      expect(deltas[0].content).toBe('Done');
      expect(deltas[0].finishReason).toBe('stop');
    });

    it('extracts tool_calls finish_reason', async () => {
      const stream = makeStream([
        'data: {"choices":[{"delta":{},"finish_reason":"tool_calls"}]}\n\n',
      ]);

      const deltas = await collectDeltas(stream);

      expect(deltas).toHaveLength(1);
      expect(deltas[0].finishReason).toBe('tool_calls');
    });
  });

  describe('reasoning_content handling', () => {
    it('parses reasoning_content delta', async () => {
      const stream = makeStream([
        'data: {"choices":[{"delta":{"reasoning_content":"Let me think..."}}]}\n\n',
      ]);

      const deltas = await collectDeltas(stream);

      expect(deltas).toHaveLength(1);
      expect(deltas[0].reasoningContent).toBe('Let me think...');
      expect(deltas[0].content).toBeNull();
    });

    it('parses mixed reasoning and content', async () => {
      const stream = makeStream([
        'data: {"choices":[{"delta":{"reasoning_content":"Thinking..."}}]}\n\n',
        'data: {"choices":[{"delta":{"content":"Answer"}}]}\n\n',
      ]);

      const deltas = await collectDeltas(stream);

      expect(deltas).toHaveLength(2);
      expect(deltas[0].reasoningContent).toBe('Thinking...');
      expect(deltas[0].content).toBeNull();
      expect(deltas[1].reasoningContent).toBeNull();
      expect(deltas[1].content).toBe('Answer');
    });
  });

  describe('tool_calls handling', () => {
    it('parses tool call delta with initial chunk', async () => {
      const stream = makeStream([
        'data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_123","type":"function","function":{"name":"get_weather"}}]}}]}\n\n',
      ]);

      const deltas = await collectDeltas(stream);

      expect(deltas).toHaveLength(1);
      expect(deltas[0].toolCalls).toHaveLength(1);
      expect(deltas[0].toolCalls![0]).toEqual({
        index: 0,
        id: 'call_123',
        type: 'function',
        function: { name: 'get_weather' },
      });
    });

    it('parses tool call argument chunks', async () => {
      const stream = makeStream([
        'data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_123","type":"function","function":{"name":"get_weather","arguments":"{\\"city\\":"}}]}}]}\n\n',
        'data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\\"NYC\\"}"}}]}}]}\n\n',
      ]);

      const deltas = await collectDeltas(stream);

      expect(deltas).toHaveLength(2);
      expect(deltas[0].toolCalls![0].function?.arguments).toBe('{"city":');
      expect(deltas[1].toolCalls![0].function?.arguments).toBe('"NYC"}');
    });

    it('parses parallel tool calls', async () => {
      const stream = makeStream([
        'data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"tool_a"}},{"index":1,"id":"call_2","type":"function","function":{"name":"tool_b"}}]}}]}\n\n',
      ]);

      const deltas = await collectDeltas(stream);

      expect(deltas).toHaveLength(1);
      expect(deltas[0].toolCalls).toHaveLength(2);
      expect(deltas[0].toolCalls![0].function?.name).toBe('tool_a');
      expect(deltas[0].toolCalls![1].function?.name).toBe('tool_b');
    });
  });

  describe('nested SSE handling', () => {
    it('handles double data: prefix (backend + llama-server)', async () => {
      const stream = makeStream([
        'data: data: {"choices":[{"delta":{"content":"Nested"}}]}\n\n',
      ]);

      const deltas = await collectDeltas(stream);

      expect(deltas).toHaveLength(1);
      expect(deltas[0].content).toBe('Nested');
    });

    it('handles nested [DONE]', async () => {
      const stream = makeStream([
        'data: {"choices":[{"delta":{"content":"Hi"}}]}\n\n',
        'data: data: [DONE]\n\n',
      ]);

      const deltas = await collectDeltas(stream);

      expect(deltas).toHaveLength(1);
      expect(deltas[0].content).toBe('Hi');
    });
  });

  describe('edge cases', () => {
    it('skips empty lines', async () => {
      const stream = makeStream([
        '\n\n',
        'data: {"choices":[{"delta":{"content":"Hi"}}]}\n\n',
        '\n',
      ]);

      const deltas = await collectDeltas(stream);

      expect(deltas).toHaveLength(1);
      expect(deltas[0].content).toBe('Hi');
    });

    it('skips non-JSON lines (comments)', async () => {
      const stream = makeStream([
        ': keep-alive\n',
        'data: {"choices":[{"delta":{"content":"Hi"}}]}\n\n',
      ]);

      const deltas = await collectDeltas(stream);

      expect(deltas).toHaveLength(1);
      expect(deltas[0].content).toBe('Hi');
    });

    it('skips malformed JSON', async () => {
      const stream = makeStream([
        'data: {invalid json}\n\n',
        'data: {"choices":[{"delta":{"content":"Valid"}}]}\n\n',
      ]);

      const deltas = await collectDeltas(stream);

      expect(deltas).toHaveLength(1);
      expect(deltas[0].content).toBe('Valid');
    });

    it('skips chunks with no content or tool calls', async () => {
      const stream = makeStream([
        'data: {"choices":[{"delta":{}}]}\n\n',
        'data: {"choices":[{"delta":{"content":"Hi"}}]}\n\n',
      ]);

      const deltas = await collectDeltas(stream);

      expect(deltas).toHaveLength(1);
      expect(deltas[0].content).toBe('Hi');
    });

    it('handles empty stream', async () => {
      const stream = makeStream([]);

      const deltas = await collectDeltas(stream);

      expect(deltas).toHaveLength(0);
    });
  });
});
