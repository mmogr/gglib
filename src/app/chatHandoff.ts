/**
 * What a chat hands to the chat it becomes: on a model switch, the
 * conversation that was open and the unsent text and images in the composer;
 * on an Open chat, the port the model was just served on, which the server
 * registry may not have heard yet.
 *
 * A module store rather than history state: the draft carries `File`s. The
 * navigation that sets one marks its history entry (`HANDED`), and only a
 * page landing on that entry reads it, so a later visit to the same address
 * by a link, a reload or Back finds nothing. Peeked, not taken, because a
 * page's first render can run twice in development; the page takes it once
 * it has committed, never from an effect's cleanup, which development also
 * runs on a page that stays.
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

/** The history state a navigation that set a handoff carries. */
export const HANDED = { handoff: true } as const;

/** Whether `state`, a location's, is a navigation that set a handoff. */
export function wasHanded(state: unknown): boolean {
  return typeof state === 'object' && state !== null && (state as { handoff?: unknown }).handoff === true;
}

let pending: ChatHandoff | null = null;

export function setChatHandoff(handoff: ChatHandoff): void {
  pending = handoff;
}

/** The handoff for `modelId`, if one waits. */
export function peekChatHandoff(modelId: number): ChatHandoff | null {
  return pending?.modelId === modelId ? pending : null;
}

/** Drop the handoff for `modelId`, once its page has it. */
export function clearChatHandoff(modelId: number): void {
  if (pending?.modelId === modelId) pending = null;
}
