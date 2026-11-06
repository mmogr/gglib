/**
 * What a chat hands to the chat it becomes: on a model switch, the
 * conversation that was open and the unsent text and images in the composer;
 * on an Open chat, the port the model was just served on, which the server
 * registry may not have heard yet.
 *
 * A module store rather than history state: the draft carries `File`s, and
 * what crosses a navigation is read once by the page that lands. Peeked, not
 * taken, because a page's first render can run twice in development; the
 * landing page clears it when it leaves.
 *
 * @module app/chatHandoff
 */

import type { ChatDraft } from '../types/messages';

export interface ChatHandoff {
  modelId: number;
  /** The port the model answers on, from the start that just finished. */
  port?: number;
  /** The model's name, for a page the registries have not caught up with. */
  modelName?: string;
  /** The conversation to open with: the one open before a model switch. */
  conversationId?: number | null;
  /** The unsent text and images to put back in the composer. */
  draft?: ChatDraft;
}

let pending: ChatHandoff | null = null;

export function setChatHandoff(handoff: ChatHandoff): void {
  pending = handoff;
}

/** The handoff for `modelId`, if one waits. */
export function peekChatHandoff(modelId: number): ChatHandoff | null {
  return pending?.modelId === modelId ? pending : null;
}

/** Drop the handoff for `modelId`, once its page has left. */
export function clearChatHandoff(modelId: number): void {
  if (pending?.modelId === modelId) pending = null;
}
