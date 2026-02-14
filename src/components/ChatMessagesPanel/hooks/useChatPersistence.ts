import { useEffect, useRef, useCallback } from 'react';
import type { ThreadRuntime, ThreadMessageLike } from '@assistant-ui/react';
import { appLogger } from '../../../services/platform';
import { getMessages, saveMessage, deleteMessage } from '../../../services/clients/chat';
import type { ConversationSummary, ChatMessage, ChatMessageMetadata } from '../../../services/clients/chat';
import { threadMessageToTranscriptMarkdown, extractNonTextContentParts, reconstructContent } from '../../../utils/messages';
import type { SerializableContentPart } from '../../../utils/messages';

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
 * Hook that handles message persistence to/from the database.
 * 
 * Responsibilities:
 * - Hydrates messages from DB when conversation changes
 * - Persists new messages as they're added
 * - Detects message edits and cascades deletes appropriately
 * - Prevents race conditions during persist operations
 */
export function useChatPersistence({
  threadRuntime,
  activeConversationId,
  activeConversation,
  persistedMessageIds,
  syncConversations,
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
        const messages = await getMessages(activeConversationId);
        if (cancelled) return;

        const prompt = activeConversation?.system_prompt?.trim();
        const systemPromptMessage: ThreadMessageLike[] = prompt && activeConversation
          ? [{
              id: `system-${activeConversation.id}`,
              role: 'system',
              content: [{ type: 'text' as const, text: prompt }],
              createdAt: new Date(activeConversation.created_at),
            }]
          : [];

        const initialMessages: ThreadMessageLike[] = [
          ...systemPromptMessage,
          ...messages.map<ThreadMessageLike>((message) => {
            // Restore metadata including research state for deep research messages
            const metadata = message.metadata;
            const isDeepResearch = metadata?.isDeepResearch === true;

            // Reconstruct structured content from metadata if available
            const storedParts = metadata?.contentParts as SerializableContentPart[] | undefined;
            const content = reconstructContent(message.content, storedParts);
            
            return {
              id: `db-${message.id}`,
              role: message.role,
              content,
              createdAt: new Date(message.created_at),
              // Include metadata with dbId for future updates and research state for rendering
              metadata: isDeepResearch
                ? {
                    custom: {
                      dbId: message.id,
                      conversationId: message.conversation_id,
                      isDeepResearch: true,
                      researchState: metadata.researchState,
                    },
                  }
                : {
                    custom: {
                      dbId: message.id,
                      conversationId: message.conversation_id,
                    },
                  },
            };
          }),
        ];

        // Build position -> DB ID mapping for edit detection and delete counting
        // Position 0 may be system message, so we track from the actual DB messages
        dbIdByPosition.current.clear();
        const systemOffset = systemPromptMessage.length;
        messages.forEach((msg, idx) => {
          dbIdByPosition.current.set(systemOffset + idx, msg.id);
        });

        const seededIds = initialMessages
          .map((msg) => msg.id)
          .filter((value): value is string => Boolean(value));
        persistedMessageIds.current = new Set(seededIds);
        threadRuntime.reset(initialMessages);
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
  }, [
    threadRuntime,
    activeConversationId,
    activeConversation?.id,
    activeConversation?.system_prompt,
    activeConversation?.created_at,
    setChatError,
    persistedMessageIds,
  ]);

  // Effect: Persist new messages and handle edit detection
  useEffect(() => {
    if (!threadRuntime || !activeConversationId) return;

    const unsubscribe = threadRuntime.subscribe(async () => {
      // Prevent concurrent persist operations
      if (isPersistingRef.current) return;
      
      const state = threadRuntime.getState();
      const messages = state.messages;
      
      for (let i = 0; i < messages.length; i++) {
        const message = messages[i];
        
        if (persistedMessageIds.current.has(message.id)) continue;
        if (message.role === 'assistant' && message.status?.type !== 'complete') continue;
        if (message.role === 'system') continue; // System messages handled separately

        const text = threadMessageToTranscriptMarkdown(message);
        const nonTextParts = extractNonTextContentParts(message);

        // Skip truly empty messages (no text AND no structured content)
        if (!text.trim() && nonTextParts.length === 0) continue;

        isPersistingRef.current = true;
        
        try {
          // Check if this is an edit: a new message at a position that already has a DB entry
          // This happens when LocalRuntime creates a new branch from an edit
          if (message.role === 'user' && dbIdByPosition.current.has(i)) {
            const existingDbId = dbIdByPosition.current.get(i)!;
            
            // Cascade delete from this position onwards in DB
            await deleteMessage(existingDbId);
            
            // Clear stale position mappings from this point forward
            for (const [pos] of dbIdByPosition.current) {
              if (pos >= i) {
                dbIdByPosition.current.delete(pos);
              }
            }
          }

          // Build metadata for non-text content parts (tool-call, audio, file, image)
          const saveMetadata: ChatMessageMetadata | null =
            nonTextParts.length > 0 ? { contentParts: nonTextParts } : null;

          // Save the new message
          const newDbId = await saveMessage(
            activeConversationId,
            message.role as ChatMessage['role'],
            text,
            saveMetadata,
          );
          
          // Update position mapping for the new message
          dbIdByPosition.current.set(i, newDbId);
          persistedMessageIds.current.add(message.id);
          
          await syncConversations({ silent: true });
        } catch (error) {
          appLogger.error('hook.ui', 'Failed to persist message', { error, conversationId: activeConversationId });
        } finally {
          isPersistingRef.current = false;
        }
      }
    });

    return unsubscribe;
  }, [threadRuntime, activeConversationId, persistedMessageIds, syncConversations]);

  return {
    isLoading: isLoadingRef.current,
    isPersisting: isPersistingRef.current,
    dbIdByPosition,
  };
}

/**
 * Hook for handling message deletion with cascade.
 * Separated from persistence for cleaner mental model.
 */
export interface UseMessageDeleteOptions {
  threadRuntime: ThreadRuntime | null;
  activeConversationId: number | null;
  activeConversation: ConversationSummary | null;
  persistedMessageIds: React.MutableRefObject<Set<string>>;
  dbIdByPosition: React.MutableRefObject<Map<number, number>>;
  syncConversations: (options?: { preferredId?: number | null; silent?: boolean }) => Promise<void>;
  showToast: (message: string, type?: 'success' | 'error' | 'warning', duration?: number) => void;
}

export interface UseMessageDeleteResult {
  /** Initiate delete flow for a message */
  initiateDelete: (runtimeMessageId: string) => void;
  /** Execute the delete after confirmation */
  confirmDelete: () => Promise<void>;
  /** Cancel the delete operation */
  cancelDelete: () => void;
  /** ID of message pending deletion */
  deleteTargetId: string | null;
  /** Whether delete modal should be shown */
  isDeleteModalOpen: boolean;
  /** Whether delete is in progress */
  isDeleting: boolean;
  /** Count of messages that will be deleted (including cascade) */
  getSubsequentMessageCount: (runtimeMessageId: string) => number;
}

/**
 * Extract database ID from runtime message ID.
 * Runtime IDs follow the pattern "db-{id}" for hydrated messages.
 */
const extractDbId = (runtimeId: string): number | null => {
  const match = runtimeId.match(/^db-(\d+)$/);
  return match ? parseInt(match[1], 10) : null;
};

/**
 * Hook for handling message deletion with confirmation modal.
 */
export function useMessageDelete({
  threadRuntime,
  activeConversationId,
  activeConversation,
  persistedMessageIds,
  dbIdByPosition,
  syncConversations,
  showToast,
}: UseMessageDeleteOptions): UseMessageDeleteResult {
  const deleteTargetIdRef = useRef<string | null>(null);
  const isDeleteModalOpenRef = useRef(false);
  const isDeletingRef = useRef(false);

  const getSubsequentMessageCount = useCallback((runtimeMessageId: string): number => {
    if (!threadRuntime) return 1;
    
    const state = threadRuntime.getState();
    const messageIndex = state.messages.findIndex((m) => m.id === runtimeMessageId);
    if (messageIndex === -1) return 1;
    
    // Count messages from this position to the end (excluding system messages)
    let count = 0;
    for (let i = messageIndex; i < state.messages.length; i++) {
      if (state.messages[i].role !== 'system') {
        count++;
      }
    }
    return count;
  }, [threadRuntime]);

  const initiateDelete = useCallback((runtimeMessageId: string) => {
    deleteTargetIdRef.current = runtimeMessageId;
    isDeleteModalOpenRef.current = true;
  }, []);

  const cancelDelete = useCallback(() => {
    isDeleteModalOpenRef.current = false;
    deleteTargetIdRef.current = null;
  }, []);

  const confirmDelete = useCallback(async () => {
    const deleteTargetId = deleteTargetIdRef.current;
    if (!deleteTargetId || !threadRuntime || !activeConversationId) return;
    
    isDeletingRef.current = true;
    try {
      // Find the DB ID from the runtime message ID
      let dbId = extractDbId(deleteTargetId);
      
      // If not found, look up by position (for newly created messages)
      if (!dbId) {
        const state = threadRuntime.getState();
        const messages = state.messages;
        const position = messages.findIndex(m => m.id === deleteTargetId);
        if (position >= 0) {
          dbId = dbIdByPosition.current.get(position) ?? null;
        }
      }
      
      if (dbId) {
        // Delete from database (cascade deletes subsequent)
        await deleteMessage(dbId);
      } else {
        appLogger.debug('hook.ui', 'Could not find DB ID for message', { messageId: deleteTargetId });
      }
      
      // Reload messages from DB and reset runtime
      const messages = await getMessages(activeConversationId);
      
      const prompt = activeConversation?.system_prompt?.trim();
      const systemPromptMessage: ThreadMessageLike[] = prompt && activeConversation
        ? [{
            id: `system-${activeConversation.id}`,
            role: 'system',
            content: [{ type: 'text' as const, text: prompt }],
            createdAt: new Date(activeConversation.created_at),
          }]
        : [];

      const reloadedMessages: ThreadMessageLike[] = [
        ...systemPromptMessage,
        ...messages.map<ThreadMessageLike>((message) => {
          const storedParts = message.metadata?.contentParts as SerializableContentPart[] | undefined;
          return {
            id: `db-${message.id}`,
            role: message.role,
            content: reconstructContent(message.content, storedParts),
            createdAt: new Date(message.created_at),
          };
        }),
      ];

      // Rebuild position mapping
      dbIdByPosition.current.clear();
      const systemOffset = systemPromptMessage.length;
      messages.forEach((msg, idx) => {
        dbIdByPosition.current.set(systemOffset + idx, msg.id);
      });

      // Update persisted IDs and reset runtime
      const seededIds = reloadedMessages
        .map((msg) => msg.id)
        .filter((value): value is string => Boolean(value));
      persistedMessageIds.current = new Set(seededIds);
      threadRuntime.reset(reloadedMessages);
      
      await syncConversations({ silent: true });
      showToast('Message deleted', 'success');
    } catch (error) {
      appLogger.error('hook.ui', 'Failed to delete message', { error, conversationId: activeConversationId });
      showToast('Failed to delete message', 'error');
    } finally {
      isDeletingRef.current = false;
      isDeleteModalOpenRef.current = false;
      deleteTargetIdRef.current = null;
    }
  }, [threadRuntime, activeConversationId, activeConversation, persistedMessageIds, dbIdByPosition, syncConversations, showToast]);

  return {
    initiateDelete,
    confirmDelete,
    cancelDelete,
    deleteTargetId: deleteTargetIdRef.current,
    isDeleteModalOpen: isDeleteModalOpenRef.current,
    isDeleting: isDeletingRef.current,
    getSubsequentMessageCount,
  };
}
