/**
 * Tests for foldToolMessages utility.
 *
 * Verifies that CLI-persisted `tool` role DB rows are correctly folded
 * back into the preceding assistant message's `contentParts`.
 */

import { describe, it, expect } from 'vitest';
import { foldToolMessages } from '../../../src/hooks/useChatPersistence/buildLoadedMessage';
import type { ChatMessage } from '../../../src/services/clients/chat';

function makeMsg(overrides: Partial<ChatMessage> & Pick<ChatMessage, 'role' | 'content'>): ChatMessage {
  return {
    id: 1,
    conversation_id: 1,
    created_at: '2026-04-03T00:00:00Z',
    metadata: null,
    ...overrides,
  };
}

describe('foldToolMessages', () => {
  it('returns messages unchanged when there are no tool rows', () => {
    const msgs: ChatMessage[] = [
      makeMsg({ id: 1, role: 'user', content: 'Hello' }),
      makeMsg({ id: 2, role: 'assistant', content: 'Hi there' }),
    ];
    expect(foldToolMessages(msgs)).toEqual(msgs);
  });

  it('folds a single tool row into the preceding assistant', () => {
    const msgs: ChatMessage[] = [
      makeMsg({ id: 1, role: 'user', content: 'list files' }),
      makeMsg({
        id: 2,
        role: 'assistant',
        content: 'Let me check.',
        metadata: {
          tool_calls: [
            { id: 'call_1', name: 'list_directory', arguments: { path: '.' } },
          ],
        },
      }),
      makeMsg({
        id: 3,
        role: 'tool',
        content: 'src/\nlib/',
        metadata: { tool_call_id: 'call_1' },
      }),
    ];

    const result = foldToolMessages(msgs);

    // Tool row is stripped
    expect(result).toHaveLength(2);
    expect(result.every((m) => m.role !== 'tool')).toBe(true);

    // Assistant now has contentParts with the tool call + result
    const assistant = result[1];
    const parts = assistant.metadata?.contentParts as any[];
    expect(parts).toHaveLength(1);
    expect(parts[0]).toMatchObject({
      type: 'tool-call',
      toolCallId: 'call_1',
      toolName: 'list_directory',
      args: { path: '.' },
      result: 'src/\nlib/',
    });
    expect(parts[0].argsText).toBe(JSON.stringify({ path: '.' }));

    // tool_calls metadata is cleaned up
    expect(assistant.metadata?.tool_calls).toBeUndefined();
  });

  it('folds multiple tool rows into one assistant', () => {
    const msgs: ChatMessage[] = [
      makeMsg({
        id: 1,
        role: 'assistant',
        content: '',
        metadata: {
          tool_calls: [
            { id: 'a', name: 'read_file', arguments: { path: 'a.rs' } },
            { id: 'b', name: 'read_file', arguments: { path: 'b.rs' } },
          ],
        },
      }),
      makeMsg({
        id: 2,
        role: 'tool',
        content: 'contents of a',
        metadata: { tool_call_id: 'a' },
      }),
      makeMsg({
        id: 3,
        role: 'tool',
        content: 'contents of b',
        metadata: { tool_call_id: 'b' },
      }),
    ];

    const result = foldToolMessages(msgs);
    expect(result).toHaveLength(1);

    const parts = result[0].metadata?.contentParts as any[];
    expect(parts).toHaveLength(2);
    expect(parts[0].toolCallId).toBe('a');
    expect(parts[0].result).toBe('contents of a');
    expect(parts[1].toolCallId).toBe('b');
    expect(parts[1].result).toBe('contents of b');
  });

  it('handles tool call with no matching result gracefully', () => {
    const msgs: ChatMessage[] = [
      makeMsg({
        id: 1,
        role: 'assistant',
        content: 'checking...',
        metadata: {
          tool_calls: [
            { id: 'orphan', name: 'grep_search', arguments: { query: 'foo' } },
          ],
        },
      }),
      // A tool row for a *different* call_id so the fast path doesn't fire,
      // but 'orphan' has no matching result.
      makeMsg({
        id: 2,
        role: 'tool',
        content: 'unrelated',
        metadata: { tool_call_id: 'other_call' },
      }),
    ];

    const result = foldToolMessages(msgs);
    expect(result).toHaveLength(1);
    const parts = result[0].metadata?.contentParts as any[];
    expect(parts).toHaveLength(1);
    expect(parts[0].toolCallId).toBe('orphan');
    expect(parts[0]).not.toHaveProperty('result');
  });

  it('preserves existing contentParts on the assistant', () => {
    const msgs: ChatMessage[] = [
      makeMsg({
        id: 1,
        role: 'assistant',
        content: 'here is the image',
        metadata: {
          contentParts: [{ type: 'image', image: 'data:image/png;base64,...' }],
          tool_calls: [
            { id: 'c', name: 'read_file', arguments: { path: 'x' } },
          ],
        },
      }),
      makeMsg({
        id: 2,
        role: 'tool',
        content: 'file x contents',
        metadata: { tool_call_id: 'c' },
      }),
    ];

    const result = foldToolMessages(msgs);
    const parts = result[0].metadata?.contentParts as any[];
    // Existing image part + new tool-call part
    expect(parts).toHaveLength(2);
    expect(parts[0].type).toBe('image');
    expect(parts[1].type).toBe('tool-call');
  });

  it('passes through GUI-created messages unchanged', () => {
    const msgs: ChatMessage[] = [
      makeMsg({ id: 1, role: 'user', content: 'hi' }),
      makeMsg({
        id: 2,
        role: 'assistant',
        content: 'hello',
        metadata: {
          contentParts: [
            {
              type: 'tool-call',
              toolCallId: 'gui_1',
              toolName: 'web_search',
              args: { q: 'rust' },
              argsText: '{"q":"rust"}',
              result: 'found stuff',
            },
          ],
        },
      }),
    ];

    // No tool rows → fast path, returned as-is
    expect(foldToolMessages(msgs)).toEqual(msgs);
  });
});
