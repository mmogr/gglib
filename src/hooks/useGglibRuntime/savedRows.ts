/**
 * What the page asks the daemon about a conversation it shows: its saved
 * rows as thread messages, the run live in it, and the row a message is.
 *
 * @module savedRows
 */

import { getTransport } from '../../services/transport';
import type { GglibMessage } from '../../types/messages';
import type { RunInfo } from '../../types/generated/RunInfo';
import {
  buildThreadMessages,
  type ThreadConversation,
} from '../useChatPersistence/buildThreadMessages';

/** A conversation's saved rows, as the thread shows them. */
export async function loadSavedThread(
  conversationId: number,
  conversation: ThreadConversation | null,
): Promise<GglibMessage[]> {
  const rows = await getTransport().getMessages(conversationId);
  // Another conversation's prompt must not head this one.
  const own = conversation?.id === conversationId ? conversation : null;
  return buildThreadMessages(rows, own, conversationId) as GglibMessage[];
}

/** The agent run still going in `conversationId`, if there is one. */
export async function liveRunFor(conversationId: number): Promise<RunInfo | undefined> {
  const runs = await getTransport().listRuns();
  return runs.find(
    (run) =>
      run.kind === 'agent' &&
      run.conversation_id === conversationId &&
      (run.status === 'queued' || run.status === 'in_progress'),
  );
}

/** The saved row `message` shows, or null for one that is not saved. */
export function savedRowId(message: GglibMessage | undefined): number | null {
  const custom = (message?.metadata as { custom?: { dbId?: unknown } } | undefined)?.custom;
  if (typeof custom?.dbId === 'number') return custom.dbId;
  const match = /^db-(\d+)$/.exec(message?.id ?? '');
  return match ? Number(match[1]) : null;
}
