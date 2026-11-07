/**
 * "Chat" on a model: its latest conversation, or an empty chat on it.
 *
 * The inspector's Chat, the library's and the endpoint's start-then-chat,
 * the endpoint's Loaded rows, the native menu's Chat and the paired model's
 * Chat all ask here, so "Chat on a model" means one thing. This machine's
 * list is read first if nothing has read it yet.
 *
 * @module app/openChat
 */

import { chatsHere, latestOn, latestOnFar } from '../services/chatRegistry';
import { HANDED, setChatHandoff } from './chatHandoff';
import type { ModelRef } from '../types/generated/ModelRef';
import { paths } from './paths';

/** Where Chat on this machine's model `modelId` goes. */
export async function chatPathOn(modelId: number): Promise<string> {
  const latest = latestOn(await chatsHere(), modelId);
  return latest ? paths.chat(latest.id) : paths.newChat({ model: modelId });
}

/** Where Chat on the paired machine's model `far` goes: a chat kept here on it. */
export async function chatPathOnFar(far: ModelRef): Promise<string> {
  const latest = latestOnFar(await chatsHere(), far);
  return latest ? paths.chat(latest.id) : paths.newChat({ far: far.id });
}

/**
 * Open Chat on this machine's model. Given the port it answers on (a start
 * that just finished, a resident slot), that port is handed to the chat
 * route, since the server list may not have heard of the model yet.
 */
export async function openChatOn(
  navigate: (to: string, options?: { state: unknown }) => unknown,
  model: { id: number; port?: number; name?: string },
): Promise<void> {
  const to = await chatPathOn(model.id);
  if (model.port === undefined) {
    navigate(to);
    return;
  }
  setChatHandoff({ modelId: model.id, port: model.port, modelName: model.name });
  navigate(to, { state: HANDED });
}
