/**
 * Tests for threadMessageToTranscriptMarkdown utility.
 * 
 * Tests conversion of ThreadMessage parts to markdown transcript text,
 * which is the single source of truth for both rendering and persistence.
 */

import { describe, it, expect } from 'vitest';
import type { ThreadMessage } from '@assistant-ui/react';
import { threadMessageToTranscriptMarkdown } from '../../../../src/utils/messages/threadMessageToTranscriptMarkdown';

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

describe('threadMessageToTranscriptMarkdown', () => {
  describe('basic part types', () => {
    it('handles text parts', () => {
      const message = createMessage([
        { type: 'text', text: 'Hello world' },
      ]);
      expect(threadMessageToTranscriptMarkdown(message)).toBe('Hello world');
    });

    it('handles reasoning parts', () => {
      const message = createMessage([
        { type: 'reasoning', text: 'Let me think' },
      ]);
      expect(threadMessageToTranscriptMarkdown(message)).toBe('<think>\nLet me think\n</think>');
    });

    it('handles string parts (legacy)', () => {
      const message = createMessage(['Plain string content']);
      expect(threadMessageToTranscriptMarkdown(message)).toBe('Plain string content');
    });

    it('trims whitespace from parts', () => {
      const message = createMessage([
        { type: 'text', text: '  \n  Hello  \n  ' },
      ]);
      expect(threadMessageToTranscriptMarkdown(message)).toBe('Hello');
    });

    it('filters empty/whitespace-only parts', () => {
      const message = createMessage([
        { type: 'text', text: '   ' },
        { type: 'reasoning', text: '\n\t\n' },
        { type: 'text', text: 'Actual content' },
      ]);
      expect(threadMessageToTranscriptMarkdown(message)).toBe('Actual content');
    });
  });

  describe('reasoning + text combinations', () => {
    it('handles reasoning followed by text', () => {
      const message = createMessage([
        { type: 'reasoning', text: 'Thinking hard' },
        { type: 'text', text: 'Here is my answer' },
      ]);
      expect(threadMessageToTranscriptMarkdown(message)).toBe(
        '<think>\nThinking hard\n</think>\n\nHere is my answer'
      );
    });

    it('handles text followed by reasoning', () => {
      const message = createMessage([
        { type: 'text', text: 'Initial response' },
        { type: 'reasoning', text: 'Wait, let me reconsider' },
      ]);
      expect(threadMessageToTranscriptMarkdown(message)).toBe(
        'Initial response\n\n<think>\nWait, let me reconsider\n</think>'
      );
    });

    it('handles interleaved text and reasoning', () => {
      const message = createMessage([
        { type: 'reasoning', text: 'First thought' },
        { type: 'text', text: 'First answer' },
        { type: 'reasoning', text: 'Second thought' },
        { type: 'text', text: 'Second answer' },
      ]);
      expect(threadMessageToTranscriptMarkdown(message)).toBe(
        '<think>\nFirst thought\n</think>\n\nFirst answer\n\n<think>\nSecond thought\n</think>\n\nSecond answer'
      );
    });
  });

  describe('coalescing adjacent reasoning', () => {
    it('coalesces two adjacent reasoning parts', () => {
      const message = createMessage([
        { type: 'reasoning', text: 'First thought' },
        { type: 'reasoning', text: 'Second thought' },
      ]);
      expect(threadMessageToTranscriptMarkdown(message)).toBe(
        '<think>\nFirst thought\n\nSecond thought\n</think>'
      );
    });

    it('coalesces multiple adjacent reasoning parts', () => {
      const message = createMessage([
        { type: 'reasoning', text: 'One' },
        { type: 'reasoning', text: 'Two' },
        { type: 'reasoning', text: 'Three' },
      ]);
      expect(threadMessageToTranscriptMarkdown(message)).toBe(
        '<think>\nOne\n\nTwo\n\nThree\n</think>'
      );
    });

    it('does not coalesce reasoning across text boundary', () => {
      const message = createMessage([
        { type: 'reasoning', text: 'Think 1' },
        { type: 'reasoning', text: 'Think 2' },
        { type: 'text', text: 'Answer' },
        { type: 'reasoning', text: 'Think 3' },
        { type: 'reasoning', text: 'Think 4' },
      ]);
      expect(threadMessageToTranscriptMarkdown(message)).toBe(
        '<think>\nThink 1\n\nThink 2\n</think>\n\nAnswer\n\n<think>\nThink 3\n\nThink 4\n</think>'
      );
    });
  });

  describe('already-wrapped reasoning', () => {
    it('does not double-wrap reasoning with <think> tags', () => {
      const message = createMessage([
        { type: 'reasoning', text: '<think>Already wrapped</think>' },
      ]);
      expect(threadMessageToTranscriptMarkdown(message)).toBe('<think>Already wrapped</think>');
    });

    it('handles already-wrapped with leading whitespace', () => {
      const message = createMessage([
        { type: 'reasoning', text: '  \n<think>Wrapped with whitespace</think>' },
      ]);
      expect(threadMessageToTranscriptMarkdown(message)).toBe('<think>Wrapped with whitespace</think>');
    });

    it('handles mix of wrapped and unwrapped reasoning', () => {
      const message = createMessage([
        { type: 'reasoning', text: '<think>Already wrapped</think>' },
        { type: 'reasoning', text: 'Not wrapped' },
      ]);
      // Adjacent reasoning gets coalesced first, then each chunk is checked for wrapping
      // The already-wrapped chunk passes through as-is, unwrapped gets wrapped separately
      // They are joined with \n\n by coalescing
      expect(threadMessageToTranscriptMarkdown(message)).toBe(
        '<think>Already wrapped</think>\n\nNot wrapped'
      );
    });
  });

  describe('tool-call parts', () => {
    it('skips tool-call parts entirely', () => {
      const message = createMessage([
        { type: 'text', text: 'Before tool' },
        { 
          type: 'tool-call', 
          toolCallId: '123',
          toolName: 'search',
          args: { query: 'test' },
          argsText: '{"query":"test"}',
        } as any,
        { type: 'text', text: 'After tool' },
      ]);
      expect(threadMessageToTranscriptMarkdown(message)).toBe('Before tool\n\nAfter tool');
    });

    it('does not coalesce reasoning across tool-call', () => {
      const message = createMessage([
        { type: 'reasoning', text: 'Before tool' },
        { 
          type: 'tool-call',
          toolCallId: '123',
          toolName: 'search',
          args: {},
          argsText: '{}',
        } as any,
        { type: 'reasoning', text: 'After tool' },
      ]);
      // Tool-call parts emit boundary markers, preventing coalescing
      // This maintains clean "thinking → action → thinking" flow
      expect(threadMessageToTranscriptMarkdown(message)).toBe(
        '<think>\nBefore tool\n</think>\n\n<think>\nAfter tool\n</think>'
      );
    });

    it('handles reasoning + tool + text without duplication', () => {
      const message = createMessage([
        { type: 'reasoning', text: 'Let me search' },
        {
          type: 'tool-call',
          toolCallId: '123',
          toolName: 'search',
          args: { query: 'test' },
          argsText: '{"query":"test"}',
          result: { data: 'result' },
        } as any,
        { type: 'text', text: 'Based on the results' },
      ]);
      // Tool call should be completely absent from markdown
      expect(threadMessageToTranscriptMarkdown(message)).toBe(
        '<think>\nLet me search\n</think>\n\nBased on the results'
      );
    });
  });

  describe('non-text parts', () => {
    it('skips image parts', () => {
      const message = createMessage([
        { type: 'text', text: 'Look at this' },
        { type: 'image', image: 'data:image/png;base64,...' } as any,
        { type: 'text', text: 'Nice image' },
      ]);
      expect(threadMessageToTranscriptMarkdown(message)).toBe('Look at this\n\nNice image');
    });

    it('skips file parts', () => {
      const message = createMessage([
        { type: 'text', text: 'Attached file' },
        { type: 'file', filename: 'doc.pdf', data: '...', mimeType: 'application/pdf' } as any,
      ]);
      expect(threadMessageToTranscriptMarkdown(message)).toBe('Attached file');
    });

    it('skips audio parts', () => {
      const message = createMessage([
        { type: 'audio', audio: { data: '...', format: 'mp3' } } as any,
        { type: 'text', text: 'Audio transcription' },
      ]);
      expect(threadMessageToTranscriptMarkdown(message)).toBe('Audio transcription');
    });

    it('skips source parts', () => {
      const message = createMessage([
        { type: 'source', sourceType: 'url', id: '1', url: 'https://example.com' } as any,
        { type: 'text', text: 'From the source' },
      ]);
      expect(threadMessageToTranscriptMarkdown(message)).toBe('From the source');
    });

    it('skips data parts', () => {
      const message = createMessage([
        { type: 'data', name: 'metadata', data: { key: 'value' } } as any,
        { type: 'text', text: 'Content' },
      ]);
      expect(threadMessageToTranscriptMarkdown(message)).toBe('Content');
    });
  });

  describe('edge cases', () => {
    it('handles empty content array', () => {
      const message = createMessage([]);
      expect(threadMessageToTranscriptMarkdown(message)).toBe('');
    });

    it('handles content with only whitespace parts', () => {
      const message = createMessage([
        { type: 'text', text: '   \n  ' },
        { type: 'reasoning', text: '\t\t\n' },
      ]);
      expect(threadMessageToTranscriptMarkdown(message)).toBe('');
    });

    it('handles complex real-world message', () => {
      const message = createMessage([
        { type: 'reasoning', text: 'User asked about X' },
        { type: 'reasoning', text: 'I should search for information' },
        {
          type: 'tool-call',
          toolCallId: '1',
          toolName: 'search',
          args: { query: 'X' },
          argsText: '{"query":"X"}',
          result: { results: ['...'] },
        } as any,
        { type: 'reasoning', text: 'Got the results, now analyzing' },
        { type: 'text', text: 'Based on my search, here is what I found...' },
      ]);
      // Tool-call creates a boundary, separating reasoning before/after the tool
      expect(threadMessageToTranscriptMarkdown(message)).toBe(
        '<think>\nUser asked about X\n\nI should search for information\n</think>\n\n<think>\nGot the results, now analyzing\n</think>\n\nBased on my search, here is what I found...'
      );
    });

    it('preserves internal formatting in text', () => {
      const message = createMessage([
        { type: 'text', text: 'Code:\n```js\nconst x = 1;\n```\nThat\'s it.' },
      ]);
      expect(threadMessageToTranscriptMarkdown(message)).toBe('Code:\n```js\nconst x = 1;\n```\nThat\'s it.');
    });

    it('handles empty reasoning part (wrapThink returns empty string)', () => {
      const message = createMessage([
        { type: 'reasoning', text: '   ' },
        { type: 'text', text: 'Content' },
      ]);
      expect(threadMessageToTranscriptMarkdown(message)).toBe('Content');
    });
  });

  describe('duration injection', () => {
    it('injects duration into reasoning blocks when callback provided', () => {
      const message = createMessage([
        { type: 'reasoning', text: 'First thought' },
        { type: 'text', text: 'Answer' },
        { type: 'reasoning', text: 'Second thought' },
      ]);
      
      const getDuration = (msgId: string, idx: number) => {
        expect(msgId).toBe('test-id');
        if (idx === 0) return 1.234;
        if (idx === 1) return 5.678;
        return undefined;
      };
      
      const result = threadMessageToTranscriptMarkdown(message, {
        getDurationForSegment: getDuration
      });
      
      expect(result).toBe(
        '<think duration="1.2">\nFirst thought\n</think>\n\n' +
        'Answer\n\n' +
        '<think duration="5.7">\nSecond thought\n</think>'
      );
    });

    it('skips duration attribute when callback returns undefined', () => {
      const message = createMessage([
        { type: 'reasoning', text: 'Thought' },
      ]);
      
      const getDuration = () => undefined;
      
      const result = threadMessageToTranscriptMarkdown(message, {
        getDurationForSegment: getDuration
      });
      
      expect(result).toBe('<think>\nThought\n</think>');
    });

    it('handles duration injection with tool-call boundaries', () => {
      const message = createMessage([
        { type: 'reasoning', text: 'Before tool' },
        { type: 'tool-call', toolCallId: 'call_1', toolName: 'test', argsText: '{}' },
        { type: 'reasoning', text: 'After tool' },
      ]);
      
      const getDuration = (_: string, idx: number) => {
        if (idx === 0) return 2.5;
        if (idx === 1) return 1.8;
        return undefined;
      };
      
      const result = threadMessageToTranscriptMarkdown(message, {
        getDurationForSegment: getDuration
      });
      
      expect(result).toBe(
        '<think duration="2.5">\nBefore tool\n</think>\n\n' +
        '<think duration="1.8">\nAfter tool\n</think>'
      );
    });

    it('works without options (backward compatibility)', () => {
      const message = createMessage([
        { type: 'reasoning', text: 'Thought' },
        { type: 'text', text: 'Text' },
      ]);
      
      const result = threadMessageToTranscriptMarkdown(message);
      
      expect(result).toBe('<think>\nThought\n</think>\n\nText');
    });

    it('increments segment index correctly across multiple reasoning blocks', () => {
      const message = createMessage([
        { type: 'reasoning', text: 'A' },
        { type: 'reasoning', text: 'B' }, // Adjacent, coalesced
        { type: 'text', text: 'X' },
        { type: 'reasoning', text: 'C' },
        { type: 'tool-call', toolCallId: 'call_1', toolName: 'test', argsText: '{}' },
        { type: 'reasoning', text: 'D' },
      ]);
      
      const indices: number[] = [];
      const getDuration = (_: string, idx: number) => {
        indices.push(idx);
        return idx * 1.1;
      };
      
      threadMessageToTranscriptMarkdown(message, {
        getDurationForSegment: getDuration
      });
      
      // Segments: [A+B], [C], [D] = indices [0, 1, 2]
      expect(indices).toEqual([0, 1, 2]);
    });
  });
});
