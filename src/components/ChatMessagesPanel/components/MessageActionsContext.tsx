import { createContext } from 'react';

/**
 * Context for message actions (delete, etc.) - allows child message bubbles
 * to trigger actions handled by the parent ChatMessagesPanel.
 */
export interface MessageActionsContextValue {
  onDeleteMessage: (runtimeMessageId: string) => void;
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
