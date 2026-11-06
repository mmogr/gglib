/**
 * "Chats" in the sidebar opens the most recent chat, until the chat list
 * lives there (C1).
 *
 * The daemon's conversation list is read and the newest one with a model
 * is opened on that model, handed to the chat route as a switch is (the
 * conversation named, the navigation marked as handing it), so the chat
 * opens on it rather than on a new thread. A model that is not loaded sends
 * the address to its inspector, where Load is, as the chat route always
 * does. No chats yet, or a list that cannot be read, goes to the library.
 *
 * @module sidebar/openLatestChat
 */

import type { NavigateFunction } from 'react-router';
import { HANDED, setChatHandoff } from '../app/chatHandoff';
import { paths } from '../app/paths';
import { listConversations } from '../services/transport/api/chat';

/** Where "Chats" went: the chat it opened, or the library. */
export type LatestChat = { kind: 'chat'; conversationId: number; modelId: number } | { kind: 'none' };

/** The newest conversation that names a model, by its last update. */
export async function latestChat(): Promise<LatestChat> {
  let chats;
  try {
    chats = await listConversations();
  } catch {
    return { kind: 'none' };
  }
  const withModel = chats.filter((c): c is typeof c & { model_id: number } => c.model_id !== null);
  withModel.sort((a, b) => b.updated_at.localeCompare(a.updated_at));
  const latest = withModel[0];
  return latest === undefined
    ? { kind: 'none' }
    : { kind: 'chat', conversationId: latest.id, modelId: latest.model_id };
}

export async function openLatestChat(navigate: NavigateFunction): Promise<LatestChat> {
  const latest = await latestChat();
  if (latest.kind === 'none') {
    navigate(paths.library());
    return latest;
  }
  setChatHandoff({ modelId: latest.modelId, conversationId: latest.conversationId });
  navigate(paths.chat(latest.modelId), { state: HANDED });
  return latest;
}
