import { useEffect, useRef, useState } from 'react';
import type { ThreadRuntime } from '@assistant-ui/react';
import { getTransport } from '../../../services/transport';
import type { ConversationSummary } from '../../../services/transport';
import { buildThreadMessages } from './buildThreadMessages';

/**
 * Options for the useThreadHydration hook.
 */
export interface UseThreadHydrationOptions {
  /** The thread runtime from @assistant-ui/react */
  threadRuntime: ThreadRuntime | null;
  /** Currently active conversation ID */
  activeConversationId: number | null;
  /** Active conversation details (for system prompt) */
  activeConversation: ConversationSummary | null;
  /** Ref to track which message IDs have been persisted */
  persistedMessageIds: React.MutableRefObject<Set<string>>;
  /** Callback to set error state */
  setChatError: (error: string | null) => void;
}

/**
 * Result returned by useThreadHydration.
 */
export interface UseThreadHydrationResult {
  /** Whether messages are currently being loaded from the database */
  isLoading: boolean;
  /** Map of runtime message position -> database ID (for edit detection) */
  dbIdByPosition: React.MutableRefObject<Map<number, number>>;
}

/**
 * Hydrates the thread runtime from the database when the conversation changes.
 *
 * Responsibilities:
 * - Loads messages from the DB and resets the runtime with them
 * - Maintains the position -> DB ID map used for edit and delete lookups
 *
 * Saving is **not** one of them. This was `useChatPersistence` and did both,
 * until the duplicate-save bug forced the persist effect out to the
 * hooks-level `useChatPersistence` in `src/hooks/useChatPersistence/`, which
 * now owns writes exclusively. What was left kept the old name, a
 * never-written `isPersisting` in its result, and a `syncConversations`
 * parameter it ignored — a hook that read as though it still saved.
 *
 * `isLoading` is state rather than a ref for the same reason: a ref set inside
 * the effect cannot re-render, so the loading branch it gates was unreachable
 * from the moment saving moved out.
 */
export function useThreadHydration({
  threadRuntime,
  activeConversationId,
  activeConversation,
  persistedMessageIds,
  setChatError,
}: UseThreadHydrationOptions): UseThreadHydrationResult {
  // Position tracking: maps runtime message index -> DB message ID
  // Used to detect edits and calculate cascade delete counts
  const dbIdByPosition = useRef<Map<number, number>>(new Map());

  const [isLoading, setIsLoading] = useState(false);

  // Effect: Hydrate messages from DB when conversation changes
  useEffect(() => {
    if (!threadRuntime || !activeConversationId) {
      return;
    }

    let cancelled = false;
    setIsLoading(true);
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
          setIsLoading(false);
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

  return { isLoading, dbIdByPosition };
}
