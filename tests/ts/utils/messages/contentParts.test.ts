/**
 * Tests for contentParts utility.
 *
 * Tests extraction of non-text content parts from ThreadMessages
 * and reconstruction of content from stored parts + text.
 */

import { describe, it, expect } from 'vitest';
import type { ThreadMessage } from '@assistant-ui/react';
import {
  extractNonTextContentParts,
  hasNonTextContent,
  reconstructContent,
} from '../../../../src/utils/messages/contentParts';

// Helper to create a mock ThreadMessage
function createMessage(content: any): ThreadMessage {
  return {
    id: 'test-id',
    role: 'assistant',
    createdAt: new Date(),
    content: content as any,
    status: { type: 'complete', reason: 'stop' },
    metadata: {
      unstable_state: null,
      unstable_annotations: [],
      unstable_data: [],
      steps: [],
      custom: {},
    },
  } as any as ThreadMessage;
}

// ============================================================================
// extractNonTextContentParts
// ============================================================================

describe('extractNonTextContentParts', () => {
  it('returns empty array for text-only message', () => {
    const message = createMessage([
      { type: 'text', text: 'Hello world' },
    ]);
    expect(extractNonTextContentParts(message)).toEqual([]);
  });

  it('returns empty array for reasoning-only message', () => {
    const message = createMessage([
      { type: 'reasoning', text: 'Let me think...' },
    ]);
    expect(extractNonTextContentParts(message)).toEqual([]);
  });

  it('extracts tool-call parts', () => {
    const message = createMessage([
      {
        type: 'tool-call',
        toolCallId: 'tc-1',
        toolName: 'get_weather',
        args: { location: 'Paris' },
      },
    ]);
    const parts = extractNonTextContentParts(message);
    expect(parts).toEqual([
      {
        type: 'tool-call',
        toolCallId: 'tc-1',
        toolName: 'get_weather',
        args: { location: 'Paris' },
      },
    ]);
  });

  it('extracts tool-call with result and isError', () => {
    const message = createMessage([
      {
        type: 'tool-call',
        toolCallId: 'tc-2',
        toolName: 'search',
        args: { query: 'test' },
        argsText: '{"query":"test"}',
        result: { items: [1, 2, 3] },
        isError: false,
      },
    ]);
    const parts = extractNonTextContentParts(message);
    expect(parts).toHaveLength(1);
    expect(parts[0]).toEqual({
      type: 'tool-call',
      toolCallId: 'tc-2',
      toolName: 'search',
      args: { query: 'test' },
      argsText: '{"query":"test"}',
      result: { items: [1, 2, 3] },
      isError: false,
    });
  });

  it('extracts tool-call with missing optional fields', () => {
    const message = createMessage([
      {
        type: 'tool-call',
        toolCallId: 'tc-3',
        toolName: 'noop',
      },
    ]);
    const parts = extractNonTextContentParts(message);
    expect(parts).toEqual([
      {
        type: 'tool-call',
        toolCallId: 'tc-3',
        toolName: 'noop',
      },
    ]);
    // Should NOT have args, argsText, result, or isError keys
    expect(parts[0]).not.toHaveProperty('args');
    expect(parts[0]).not.toHaveProperty('result');
  });

  it('extracts multiple tool-calls mixed with text', () => {
    const message = createMessage([
      { type: 'text', text: 'Let me search for that.' },
      { type: 'tool-call', toolCallId: 'tc-a', toolName: 'search', args: { q: 'cats' } },
      { type: 'text', text: 'Found results.' },
      { type: 'tool-call', toolCallId: 'tc-b', toolName: 'fetch', args: { url: 'http://example.com' } },
    ]);
    const parts = extractNonTextContentParts(message);
    expect(parts).toHaveLength(2);
    expect(parts[0].type).toBe('tool-call');
    expect(parts[1].type).toBe('tool-call');
  });

  it('extracts image parts', () => {
    const message = createMessage([
      { type: 'image', image: 'data:image/png;base64,abc123' },
    ]);
    const parts = extractNonTextContentParts(message);
    expect(parts).toEqual([
      { type: 'image', image: 'data:image/png;base64,abc123' },
    ]);
  });

  it('extracts file parts', () => {
    const message = createMessage([
      { type: 'file', data: 'base64data==', mimeType: 'application/pdf' },
    ]);
    const parts = extractNonTextContentParts(message);
    expect(parts).toEqual([
      { type: 'file', data: 'base64data==', mimeType: 'application/pdf' },
    ]);
  });

  it('extracts audio parts with nested audio object', () => {
    const message = createMessage([
      { type: 'audio', audio: { data: 'audiodata==', format: 'wav' } },
    ]);
    const parts = extractNonTextContentParts(message);
    expect(parts).toEqual([
      { type: 'audio', data: 'audiodata==', format: 'wav' },
    ]);
  });

  it('skips string parts, source, data, and unknown types', () => {
    const message = createMessage([
      'legacy string content',
      { type: 'source', source: 'http://example.com' },
      { type: 'data', data: { foo: 'bar' } },
      { type: 'unknown-future-type', value: 42 },
    ]);
    expect(extractNonTextContentParts(message)).toEqual([]);
  });
});

// ============================================================================
// hasNonTextContent
// ============================================================================

describe('hasNonTextContent', () => {
  it('returns false for text-only message', () => {
    const message = createMessage([{ type: 'text', text: 'Hello' }]);
    expect(hasNonTextContent(message)).toBe(false);
  });

  it('returns false for empty content', () => {
    const message = createMessage([]);
    expect(hasNonTextContent(message)).toBe(false);
  });

  it('returns true when tool-call present', () => {
    const message = createMessage([
      { type: 'tool-call', toolCallId: 'tc-1', toolName: 'test' },
    ]);
    expect(hasNonTextContent(message)).toBe(true);
  });

  it('returns true when image present', () => {
    const message = createMessage([
      { type: 'text', text: 'Look at this' },
      { type: 'image', image: 'data:image/png;base64,abc' },
    ]);
    expect(hasNonTextContent(message)).toBe(true);
  });

  it('returns true when audio present', () => {
    const message = createMessage([
      { type: 'audio', audio: { data: 'abc', format: 'wav' } },
    ]);
    expect(hasNonTextContent(message)).toBe(true);
  });

  it('returns true when file present', () => {
    const message = createMessage([
      { type: 'file', data: 'abc', mimeType: 'text/plain' },
    ]);
    expect(hasNonTextContent(message)).toBe(true);
  });

  it('returns false for reasoning-only', () => {
    const message = createMessage([
      { type: 'reasoning', text: 'Thinking...' },
    ]);
    expect(hasNonTextContent(message)).toBe(false);
  });
});

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

  it('reconstructs audio parts', () => {
    const parts = [
      { type: 'audio' as const, data: 'audiodata==', format: 'wav' },
    ];
    const result = reconstructContent('', parts) as any[];
    expect(result).toHaveLength(1);
    expect(result[0].type).toBe('audio');
    expect(result[0].audio).toEqual({ data: 'audiodata==', format: 'wav' });
  });

  it('reconstructs file parts', () => {
    const parts = [
      { type: 'file' as const, data: 'base64==', mimeType: 'application/pdf' },
    ];
    const result = reconstructContent('', parts) as any[];
    expect(result).toHaveLength(1);
    expect(result[0].type).toBe('file');
    expect(result[0].data).toBe('base64==');
    expect(result[0].mimeType).toBe('application/pdf');
  });

  it('reconstructs image parts', () => {
    const parts = [
      { type: 'image' as const, image: 'data:image/png;base64,abc' },
    ];
    const result = reconstructContent('', parts) as any[];
    expect(result).toHaveLength(1);
    expect(result[0].type).toBe('image');
    expect(result[0].image).toBe('data:image/png;base64,abc');
  });

  it('handles mixed content: text + multiple tool-calls + image', () => {
    const parts = [
      { type: 'tool-call' as const, toolCallId: 'tc-1', toolName: 'search', args: { q: 'cats' } },
      { type: 'tool-call' as const, toolCallId: 'tc-2', toolName: 'fetch', args: { url: 'http://x.com' } },
      { type: 'image' as const, image: 'data:image/png;base64,abc' },
    ];
    const result = reconstructContent('Here are the results:', parts) as any[];
    expect(result).toHaveLength(4); // text + 2 tool-calls + image
    expect(result[0].type).toBe('text');
    expect(result[1].type).toBe('tool-call');
    expect(result[2].type).toBe('tool-call');
    expect(result[3].type).toBe('image');
  });

  it('round-trips: extract → reconstruct preserves tool-call data', () => {
    const message = createMessage([
      { type: 'text', text: 'Calling tool...' },
      {
        type: 'tool-call',
        toolCallId: 'tc-rt',
        toolName: 'calculator',
        args: { expression: '2+2' },
        result: 4,
      },
    ]);

    const extracted = extractNonTextContentParts(message);
    const text = 'Calling tool...'; // simulating what threadMessageToTranscriptMarkdown returns
    const reconstructed = reconstructContent(text, extracted) as any[];

    expect(reconstructed).toHaveLength(2);
    expect(reconstructed[0]).toEqual({ type: 'text', text: 'Calling tool...' });
    expect(reconstructed[1].type).toBe('tool-call');
    expect(reconstructed[1].toolName).toBe('calculator');
    expect(reconstructed[1].result).toBe(4);
  });

  it('round-trips: tool-call-only message (the critical bug case)', () => {
    // This is the exact scenario that was causing the alternation error:
    // An assistant message with ONLY tool-calls and no text.
    const message = createMessage([
      { type: 'tool-call', toolCallId: 'tc-bug', toolName: 'get_weather', args: { loc: 'NYC' }, result: '72°F' },
    ]);

    const extracted = extractNonTextContentParts(message);
    expect(extracted).toHaveLength(1);

    // Text would be '' from threadMessageToTranscriptMarkdown
    const text = '';

    // OLD behavior: if (!text.trim()) continue; → DROPPED! ❌
    // NEW behavior: nonTextParts.length > 0, so we persist it ✅
    expect(extracted.length > 0).toBe(true);

    const reconstructed = reconstructContent(text, extracted) as any[];
    expect(reconstructed).toHaveLength(1);
    expect(reconstructed[0].type).toBe('tool-call');
    expect(reconstructed[0].toolName).toBe('get_weather');
    expect(reconstructed[0].result).toBe('72°F');
  });
});
