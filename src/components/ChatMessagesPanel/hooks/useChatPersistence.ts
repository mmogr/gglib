import { useEffect, useRef } from 'react';
import type { ThreadRuntime } from '@assistant-ui/react';
import { getTransport } from '../../../services/transport';
import type { ConversationSummary } from '../../../services/transport';
import { buildThreadMessages } from './buildThreadMessages';

/**
 * Options for the useChatPersistence hook.
 */
export interface UseChatPersistenceOptions {
  /** The thread runtime from @assistant-ui/react */
  threadRuntime: ThreadRuntime | null;
  /** Currently active conversation ID */
  activeConversationId: number | null;
  /** Active conversation details (for system prompt) */
  activeConversation: ConversationSummary | null;
  /** Ref to track which message IDs have been persisted */
  persistedMessageIds: React.MutableRefObject<Set<string>>;
  /** Callback to sync conversations list (usually silent refresh) */
  syncConversations: (options?: { preferredId?: number | null; silent?: boolean }) => Promise<void>;
  /** Callback to set error state */
  setChatError: (error: string | null) => void;
}

/**
 * Result returned by useChatPersistence.
 */
export interface UseChatPersistenceResult {
  /** Whether messages are currently being loaded */
  isLoading: boolean;
  /** Whether a persist operation is in progress */
  isPersisting: boolean;
  /** Map of runtime message position -> database ID (for edit detection) */
  dbIdByPosition: React.MutableRefObject<Map<number, number>>;
}

/**
 * Hook that hydrates the thread runtime from the database.
 *
 * Responsibilities:
 * - Hydrates messages from DB when the conversation changes
 * - Maintains the position -> DB ID map used for edit and delete lookups
 *
 * Saving new and changed messages is handled exclusively by the hooks-level
 * useChatPersistence in `src/hooks/useChatPersistence.ts`; the persist effect
 * was removed from here to prevent duplicate saves.
 */
export function useChatPersistence({
  threadRuntime,
  activeConversationId,
  activeConversation,
  persistedMessageIds,
  syncConversations: _syncConversations,
  setChatError,
}: UseChatPersistenceOptions): UseChatPersistenceResult {
  // Position tracking: maps runtime message index -> DB message ID
  // Used to detect edits and calculate cascade delete counts
  const dbIdByPosition = useRef<Map<number, number>>(new Map());

  // Race condition protection for persist operations
  const isPersistingRef = useRef(false);

  // Loading state for hydration
  const isLoadingRef = useRef(false);

  // Effect: Hydrate messages from DB when conversation changes
  useEffect(() => {
    if (!threadRuntime || !activeConversationId) {
      return;
    }

    let cancelled = false;
    isLoadingRef.current = true;
    setChatError(null);

    const hydrate = async () => {
      try {
        const dbMessages = await getTransport().getMessages(activeConversationId);
        if (cancelled) return;

        const { messages, dbIdByPosition: positions, seededIds } = buildThreadMessages(
          dbMessages,
          activeConversation,
          activeConversationId,
        );

        dbIdByPosition.current = positions;
        persistedMessageIds.current = seededIds;
        threadRuntime.reset(messages);
      } catch (error) {
        if (!cancelled) {
          setChatError(error instanceof Error ? error.message : String(error));
        }
      } finally {
        if (!cancelled) {
          isLoadingRef.current = false;
        }
      }
    };

    hydrate();
    return () => { cancelled = true; };
    // Depends on specific activeConversation FIELDS (id, system_prompt,
    // created_at) rather than the object: re-hydrating the thread whenever
    // any other field changes (e.g. a title rename) would needlessly reload
    // and reset the runtime mid-conversation.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    threadRuntime,
    activeConversationId,
    activeConversation?.id,
    activeConversation?.system_prompt,
    activeConversation?.created_at,
    setChatError,
    persistedMessageIds,
  ]);

  return {
    isLoading: isLoadingRef.current,
    isPersisting: isPersistingRef.current,
    dbIdByPosition,
  };
}
