/**
 * Tests for contentParts utility.
 *
 * Tests reconstruction of a thread message's content from a saved row's
 * text and its stored tool calls.
 */

import { describe, it, expect } from 'vitest';
import {
  reconstructContent,
  type SerializableToolCallPart,
} from '../../../../src/utils/messages/contentParts';

// ============================================================================
// reconstructContent
// ============================================================================

describe('reconstructContent', () => {
  it('returns plain text when no content parts stored (backward compat)', () => {
    expect(reconstructContent('Hello world', null)).toBe('Hello world');
    expect(reconstructContent('Hello world', undefined)).toBe('Hello world');
    expect(reconstructContent('Hello world', [])).toBe('Hello world');
  });

  it('returns text for empty text and no parts', () => {
    expect(reconstructContent('', null)).toBe('');
  });

  it('builds parts array with text + tool-call', () => {
    const parts = [
      {
        type: 'tool-call' as const,
        toolCallId: 'tc-1',
        toolName: 'get_weather',
        args: { location: 'Paris' },
      },
    ];
    const result = reconstructContent('Let me check the weather.', parts);

    expect(Array.isArray(result)).toBe(true);
    const contentArray = result as any[];
    expect(contentArray).toHaveLength(2);
    expect(contentArray[0]).toEqual({ type: 'text', text: 'Let me check the weather.' });
    expect(contentArray[1]).toEqual({
      type: 'tool-call',
      toolCallId: 'tc-1',
      toolName: 'get_weather',
      args: { location: 'Paris' },
    });
  });

  it('builds parts array with tool-call only (empty text)', () => {
    const parts = [
      {
        type: 'tool-call' as const,
        toolCallId: 'tc-1',
        toolName: 'search',
        args: { q: 'test' },
        result: 'found 3 results',
      },
    ];
    const result = reconstructContent('', parts);

    expect(Array.isArray(result)).toBe(true);
    const contentArray = result as any[];
    // Should NOT include an empty text part
    expect(contentArray).toHaveLength(1);
    expect(contentArray[0].type).toBe('tool-call');
  });

  it('preserves tool-call result and isError', () => {
    const parts = [
      {
        type: 'tool-call' as const,
        toolCallId: 'tc-err',
        toolName: 'failing_tool',
        args: {},
        result: 'Error: not found',
        isError: true,
      },
    ];
    const result = reconstructContent('', parts);
    const contentArray = result as any[];
    expect(contentArray[0].result).toBe('Error: not found');
    expect(contentArray[0].isError).toBe(true);
  });

  it('keeps the images a tool call carries on its artifact', () => {
    const images = [{ id: 'a'.repeat(64), mime: 'image/png', width: 1024, height: 1024 }];
    const parts: SerializableToolCallPart[] = [
      { type: 'tool-call', toolCallId: 'tc-img', toolName: 'draw', result: 'drawn', artifact: { images } },
      { type: 'tool-call', toolCallId: 'tc-txt', toolName: 'echo', result: 'hi' },
    ];
    const result = reconstructContent('', parts) as any[];
    expect(result[0].artifact).toEqual({ images });
    expect(result[1]).not.toHaveProperty('artifact');
  });

  it('leaves out an artifact that holds no images', () => {
    // The column is free JSON: an artifact without an images list is not one.
    const parts = [
      { type: 'tool-call', toolCallId: 'tc-1', toolName: 'draw', artifact: { images: 'nope' } },
      { type: 'tool-call', toolCallId: 'tc-2', toolName: 'draw', artifact: { images: [] } },
    ] as unknown as SerializableToolCallPart[];
    expect(reconstructContent('', parts)).toEqual([
      { type: 'tool-call', toolCallId: 'tc-1', toolName: 'draw' },
      { type: 'tool-call', toolCallId: 'tc-2', toolName: 'draw' },
    ]);
  });

  it('keeps the tool calls in their stored order, after the text', () => {
    const parts = [
      { type: 'tool-call' as const, toolCallId: 'tc-1', toolName: 'search', args: { q: 'cats' } },
      { type: 'tool-call' as const, toolCallId: 'tc-2', toolName: 'fetch', args: { url: 'http://x.com' } },
    ];
    const result = reconstructContent('Here are the results:', parts) as any[];
    expect(result.map((p) => p.type)).toEqual(['text', 'tool-call', 'tool-call']);
    expect(result.map((p) => p.toolCallId)).toEqual([undefined, 'tc-1', 'tc-2']);
  });

  it('leaves out a stored part that is not a tool call', () => {
    // The column is free JSON, so a row can hold what the type does not.
    const parts = [
      { type: 'image', image: 'data:image/png;base64,abc' },
      { type: 'tool-call', toolCallId: 'tc-1', toolName: 'search' },
    ] as unknown as SerializableToolCallPart[];
    expect(reconstructContent('', parts)).toEqual([
      { type: 'tool-call', toolCallId: 'tc-1', toolName: 'search' },
    ]);
  });

  it('gives the text alone when no stored part is a tool call', () => {
    const parts = [
      { type: 'audio', data: 'audiodata==', format: 'wav' },
    ] as unknown as SerializableToolCallPart[];
    expect(reconstructContent('Hello', parts)).toEqual([{ type: 'text', text: 'Hello' }]);
    expect(reconstructContent('', parts)).toBe('');
  });
});
