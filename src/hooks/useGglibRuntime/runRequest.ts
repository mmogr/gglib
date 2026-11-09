/**
 * The body of this machine's turn, `PUT /api/runs/{id}?kind=agent`: the
 * conversation it is added to and the new message, nothing more. The daemon
 * rebuilds the history from what the conversation saved, and runs the turn
 * on the model, tools and limits the conversation's settings name.
 *
 * @module runRequest
 */

import type { LocalTurn } from '../../types/generated/LocalTurn';
import type { Thinking } from '../../types/generated/Thinking';

export interface TurnOptions {
  /** The conversation the daemon saves the user's message and the reply to. */
  conversationId: number;
  /** The user's message; `''` for an answer turn, and for images alone. */
  content: string;
  /** The images the message carries, by the ids their uploads answered. */
  images?: readonly string[];
  /**
   * Whether the turn answers the question the conversation already ends in,
   * as an edit, a regenerate or Retry leaves it: it then says no message,
   * and the daemon saves only the reply.
   */
  answerSaved?: boolean;
  /**
   * The conversation's Thinking choice, on the turn that changes it: `off`
   * or `default`. Left out of the body otherwise, and the daemon runs the
   * turn as the conversation remembers.
   */
  thinking?: Thinking;
  /**
   * The conversation's folder, where gglib's own file tools read, on the
   * turn that changes it: a path sets it, `""` forgets it. Left out of the
   * body otherwise, and the daemon reads the folder the conversation
   * remembers, or offers no file tools.
   */
  folder?: string;
  /** Sent with Draw pressed: the daemon offers this turn its drawing tool. */
  draw?: boolean;
  /**
   * The image model chosen for this chat's pictures, by its id. In the body
   * only on a turn that says `draw`, and on every one that does while the
   * choice stands: the daemon remembers nothing of it.
   */
  imageModel?: number;
}

/** A run id the hub accepts: 1–64 of `[A-Za-z0-9_-]`. */
export function mintRunId(): string {
  return `chat-${crypto.randomUUID()}`;
}

/** The turn's body: a key with nothing to say is left out. */
export function buildTurn(options: TurnOptions): LocalTurn {
  const images = options.images ?? [];
  return {
    conversation_id: options.conversationId,
    content: options.content,
    ...(images.length > 0 && { images: [...images] }),
    ...(options.thinking !== undefined && { thinking: options.thinking }),
    ...(options.answerSaved && { answer_saved: true }),
    ...(options.draw === true && { draw: true }),
    ...(options.draw === true && options.imageModel !== undefined && { image_model: options.imageModel }),
    ...(options.folder !== undefined && { folder: options.folder }),
  };
}
