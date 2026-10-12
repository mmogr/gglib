import { createContext } from 'react';
import type { ChatSource } from '../../../services/transport';
import type { ImageModelRow } from '../../../types/generated/ImageModelRow';

/** What a picture a tool drew is fixed or drawn again with, in the open chat. */
export interface PictureActions {
  conversationId: number;
  /** This machine's image models, to choose among. */
  models: readonly ImageModelRow[];
  /** The image model the chat has chosen, by id; `null` for the machine's own choice. */
  picked: number | null;
  /** Told once the chat holds a newer picture, to show it. */
  onKept: () => void | Promise<void>;
}

/**
 * Context for message actions (delete, etc.) - allows child message bubbles
 * to trigger actions handled by the parent ChatMessagesPanel.
 */
export interface MessageActionsContextValue {
  onDeleteMessage: (runtimeMessageId: string) => void;
  /**
   * Whose chat it is. A far chat's turns are the far machine's: none is
   * edited, regenerated or deleted from here, and none of its users is "You".
   */
  source?: ChatSource;
  /**
   * Fixing and redoing a drawn picture. Absent where neither can be done:
   * a far chat, whose pictures this machine cannot draw from, and a chat
   * with no saved conversation to keep the result in.
   */
  pictures?: PictureActions;
}

export const MessageActionsContext = createContext<MessageActionsContextValue | null>(null);

/**
 * Extract database ID from runtime message ID.
 * Runtime IDs follow the pattern "db-{id}" for hydrated messages.
 * 
 * @example
 * extractDbId("db-123") // returns 123
 * extractDbId("temp-abc") // returns null
 */
export const extractDbId = (runtimeId: string): number | null => {
  const match = runtimeId.match(/^db-(\d+)$/);
  return match ? parseInt(match[1], 10) : null;
};
