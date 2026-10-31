/**
 * A saved row as the thread shows it: an unfinished reply stays unfinished.
 *
 * The daemon marks the last assistant row of a run that ended without its
 * answer (stopped, failed, or the daemon went away) `metadata.incomplete`.
 */

import { describe, it, expect } from 'vitest';
import { buildLoadedMessage, foldToolMessages } from '../../../src/hooks/useChatPersistence/buildLoadedMessage';
import type { ChatMessage } from '../../../src/services/transport';

function row(extra: Partial<ChatMessage>): ChatMessage {
  return {
    id: 5,
    conversation_id: 1,
    role: 'assistant',
    content: 'half an ans',
    created_at: '2026-09-29T00:00:00Z',
    ...extra,
  };
}

describe('buildLoadedMessage', () => {
  it('shows an unfinished reply as a cancelled one', () => {
    const loaded = buildLoadedMessage(row({ metadata: { incomplete: true } }), 1);
    expect(loaded.status).toEqual({ type: 'incomplete', reason: 'cancelled' });
    expect(loaded.id).toBe('db-5');
  });

  it('leaves a finished reply to the runtime\'s own status', () => {
    expect(buildLoadedMessage(row({ metadata: null }), 1).status).toBeUndefined();
    expect(buildLoadedMessage(row({ metadata: { incomplete: false } }), 1).status).toBeUndefined();
  });

  it('never marks a user row, which cannot carry a status', () => {
    const loaded = buildLoadedMessage(row({ role: 'user', metadata: { incomplete: true } }), 1);
    expect(loaded.status).toBeUndefined();
  });

  it('shows how long a reply thought, as the daemon saved it', () => {
    const loaded = buildLoadedMessage(
      row({ metadata: { thinking: 'hmm', thinkingDurationSeconds: 2.5 } }),
      1,
    );
    expect(loaded.metadata?.custom).toMatchObject({ thinkingDurationSeconds: 2.5 });
    expect((loaded.content as unknown as Array<{ type: string }>)[0]).toMatchObject({ type: 'reasoning', text: 'hmm' });
  });

  it('keeps how a reply was made, only the figures saved, a saved zero as zero', () => {
    const made = (extra: Partial<ChatMessage>) =>
      (buildLoadedMessage(row(extra), 1).metadata as { custom: { made?: unknown } }).custom.made;
    expect(made({ metadata: { modelName: 'qwen3', cachedTokens: 0, junk: 7 } })).toEqual({
      modelName: 'qwen3',
      cachedTokens: 0,
    });
    expect(made({ metadata: { thinking: 'x' } })).toBeUndefined();
    expect(made({ role: 'user', metadata: { modelName: 'qwen3' } })).toBeUndefined();
  });

  it('gives a user row its saved images as complete attachments, by id with their facts', () => {
    const shot = { id: 'c'.repeat(64), mime: 'image/jpeg', width: 4032, height: 3024 };
    const loaded = buildLoadedMessage(row({ role: 'user', content: 'what is this?', images: [shot] }), 1);
    expect(loaded.attachments).toEqual([
      { id: shot.id, type: 'image', name: 'image', contentType: 'image/jpeg', status: { type: 'complete' }, content: [], stored: shot },
    ]);
  });

  it('gives a row with no images no attachments', () => {
    expect(buildLoadedMessage(row({ role: 'user', content: 'hi' }), 1).attachments).toBeUndefined();
  });

  it('shows a saved call by the name and MCP server its run showed, and an own tool with no server', () => {
    const rows = [
      row({
        id: 1,
        content: '',
        metadata: {
          tool_calls: [
            { id: 'c1', name: '3:tavily_search', arguments: { q: 'x' }, display_name: 'Tavily Search', server: 'Tavily Web Search' },
            { id: 'c2', name: 'builtin:get_current_time', arguments: {}, display_name: 'Get Current Time' },
          ],
        },
      }),
      row({ id: 2, role: 'tool', content: 'found', metadata: { tool_call_id: 'c1' } }),
      row({ id: 3, role: 'tool', content: 'noon', metadata: { tool_call_id: 'c2' } }),
    ];
    const [folded] = foldToolMessages(rows);
    const content = buildLoadedMessage(folded, 1).content as unknown as Array<Record<string, unknown>>;

    expect(content[0]).toMatchObject({ toolCallId: 'c1', displayName: 'Tavily Search', server: 'Tavily Web Search', result: 'found' });
    expect(content[1]).toEqual({
      type: 'tool-call',
      toolCallId: 'c2',
      toolName: 'builtin:get_current_time',
      args: {},
      argsText: '{}',
      result: 'noon',
      displayName: 'Get Current Time',
    });
  });
});
