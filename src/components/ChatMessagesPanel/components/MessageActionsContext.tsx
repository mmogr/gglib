import { createContext } from 'react';
import type { ChatSource } from '../../../services/transport';

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
