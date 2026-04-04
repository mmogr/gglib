/**
 * Tests for threadMessageToTranscriptMarkdown utility.
 * 
 * This function produces the answer-only text stored in the DB `content` column.
 * Reasoning parts are intentionally excluded (persisted separately in metadata).
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

    it('excludes reasoning parts (stored in metadata)', () => {
      const message = createMessage([
        { type: 'reasoning', text: 'Let me think' },
      ]);
      expect(threadMessageToTranscriptMarkdown(message)).toBe('');
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
        { type: 'reasoning', text: 'ignored' },
        { type: 'text', text: 'Actual content' },
      ]);
      expect(threadMessageToTranscriptMarkdown(message)).toBe('Actual content');
    });
  });

  describe('reasoning + text combinations', () => {
    it('outputs only text when reasoning is present', () => {
      const message = createMessage([
        { type: 'reasoning', text: 'Thinking hard' },
        { type: 'text', text: 'Here is my answer' },
      ]);
      expect(threadMessageToTranscriptMarkdown(message)).toBe('Here is my answer');
    });

    it('outputs only text from interleaved content', () => {
      const message = createMessage([
        { type: 'reasoning', text: 'First thought' },
        { type: 'text', text: 'First answer' },
        { type: 'reasoning', text: 'Second thought' },
        { type: 'text', text: 'Second answer' },
      ]);
      expect(threadMessageToTranscriptMarkdown(message)).toBe(
        'First answer\n\nSecond answer'
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

    it('handles reasoning + tool + text (only text in output)', () => {
      const message = createMessage([
        { type: 'reasoning', text: 'Let me search' },
        {
          type: 'tool-call',
          toolCallId: '123',
          toolName: 'search',
          args: { query: 'test' },
        } as any,
        { type: 'text', text: 'Based on the results' },
      ]);
      expect(threadMessageToTranscriptMarkdown(message)).toBe('Based on the results');
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

    it('handles complex real-world message (only text in output)', () => {
      const message = createMessage([
        { type: 'reasoning', text: 'User asked about X' },
        { type: 'reasoning', text: 'I should search for information' },
        {
          type: 'tool-call',
          toolCallId: '1',
          toolName: 'search',
          args: { query: 'X' },
        } as any,
        { type: 'reasoning', text: 'Got the results, now analyzing' },
        { type: 'text', text: 'Based on my search, here is what I found...' },
      ]);
      expect(threadMessageToTranscriptMarkdown(message)).toBe(
        'Based on my search, here is what I found...'
      );
    });

    it('preserves internal formatting in text', () => {
      const message = createMessage([
        { type: 'text', text: 'Code:\n```js\nconst x = 1;\n```\nThat\'s it.' },
      ]);
      expect(threadMessageToTranscriptMarkdown(message)).toBe('Code:\n```js\nconst x = 1;\n```\nThat\'s it.');
    });

    it('handles empty reasoning part', () => {
      const message = createMessage([
        { type: 'reasoning', text: '   ' },
        { type: 'text', text: 'Content' },
      ]);
      expect(threadMessageToTranscriptMarkdown(message)).toBe('Content');
    });
  });
});
