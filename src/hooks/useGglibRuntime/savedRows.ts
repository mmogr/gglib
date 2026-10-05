/**
 * What the page asks the daemon about a conversation it shows: its saved
 * rows as thread messages, the run live in it, and the row a message is.
 * A far chat's rows and live run are the far machine's, asked through this
 * machine's daemon; its rows head themselves with its own prompt, and each
 * reading of it is handed to whoever asked to see it (`onFarOpened`), since
 * the far list tells none of a chat's settings.
 *
 * @module savedRows
 */

import { getTransport, type ChatSource } from '../../services/transport';
import type { GglibMessage } from '../../types/messages';
import type { HubChatOpen } from '../../types/generated/HubChatOpen';
import type { RunInfo } from '../../types/generated/RunInfo';
import {
  buildThreadMessages,
  type ThreadConversation,
} from '../useChatPersistence/buildThreadMessages';

/**
 * A conversation's saved rows, as the thread shows them. A far chat is
 * handed to `onFarOpened` as the far machine answered it, every time.
 */
export async function loadSavedThread(
  conversationId: number,
  conversation: ThreadConversation | null,
  source: ChatSource = 'this',
  onFarOpened?: (open: HubChatOpen) => void,
): Promise<GglibMessage[]> {
  if (source === 'far') {
    const open = await getTransport().openFarChat(conversationId);
    onFarOpened?.(open);
    return buildThreadMessages(open.messages, open.conversation, conversationId) as GglibMessage[];
  }
  const rows = await getTransport().getMessages(conversationId);
  // Another conversation's prompt must not head this one.
  const own = conversation?.id === conversationId ? conversation : null;
  return buildThreadMessages(rows, own, conversationId) as GglibMessage[];
}

/** The agent run still going in `conversationId`, if there is one. */
export async function liveRunFor(
  conversationId: number,
  source: ChatSource = 'this',
): Promise<Pick<RunInfo, 'id'> | undefined> {
  if (source === 'far') {
    const chat = (await getTransport().listFarChats()).find((c) => c.id === conversationId);
    return chat?.live_run ? { id: chat.live_run } : undefined;
  }
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
