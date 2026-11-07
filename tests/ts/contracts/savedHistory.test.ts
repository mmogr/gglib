/**
 * The history the daemon rebuilds for a turn is the one the page sent.
 *
 * `contracts/chats/history.json` is written by `gglib-core`'s history
 * contract tests: a chat's prompt and saved rows, as the thread route gives
 * them, beside `saved_history`'s messages. Until the page sent only its new
 * message, it sent the whole history, rebuilt from the rows it had loaded
 * (`buildThreadMessages`, then the conversion kept in `pageHistory.ts`).
 * Replayed here, every case must give the daemon's messages: the switch to
 * the daemon's history changed nothing a model is sent.
 */

import { describe, it, expect } from 'vitest';
import type { ChatMessage } from '../../../src/services/transport';
import type { AgentMessage } from '../../../src/types/generated/AgentMessage';
import type { GglibMessage } from '../../../src/types/messages';
import { buildThreadMessages } from '../../../src/hooks/useChatPersistence/buildThreadMessages';
import { convertToWireMessages } from './pageHistory';
import { rust } from './rustSource';

interface Case {
  name: string;
  system_prompt: string | null;
  rows: ChatMessage[];
  history: AgentMessage[];
}

const CONTRACT = JSON.parse(rust('contracts/chats/history.json')) as { cases: Case[] };

const CONVERSATION = 7;

/** What the page sent for a chat it had loaded. */
function pageSent(c: Case): AgentMessage[] {
  const conversation = { id: CONVERSATION, system_prompt: c.system_prompt, created_at: '2026-11-08 09:00:00' };
  return convertToWireMessages(buildThreadMessages(c.rows, conversation, CONVERSATION) as GglibMessage[]);
}

describe('saved history contract', () => {
  it('has the cases the switch was proved on', () => {
    expect(CONTRACT.cases.map((c) => c.name)).toEqual([
      'a prompt, a question and its reasoned answer',
      'no prompt',
      'a blank prompt',
      'tool calls, folded on the page, and a drawn picture',
      'an unfinished reply, stopped mid-call',
      'a reply stopped before it said anything',
      "images on the user's messages, one with no words",
      'a chat that ends in its question',
      'a system row is never sent',
    ]);
  });

  it.each(CONTRACT.cases.map((c) => [c.name, c] as const))('%s: the page sent what the daemon rebuilds', (_name, c) => {
    expect(pageSent(c)).toEqual(c.history);
  });

  it('sends the prompt trimmed, first, and no thinking', () => {
    const [answered] = CONTRACT.cases;
    expect(pageSent(answered)).toEqual([
      { role: 'system', content: 'Be brief.' },
      { role: 'user', content: 'What is 2 + 2?' },
      { role: 'assistant', content: '4' },
    ]);
  });
});
