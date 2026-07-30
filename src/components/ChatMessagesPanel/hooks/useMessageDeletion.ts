import { useCallback, useState } from 'react';
import type { ThreadRuntime } from '@assistant-ui/react';
import { appLogger } from '../../../services/platform';
import { getTransport } from '../../../services/transport';
import type { ConversationSummary } from '../../../services/transport';
import { extractDbId } from '../components/MessageActionsContext';
import { buildThreadMessages } from './buildThreadMessages';
import type { ToastType } from '../../Toast';

export interface UseMessageDeletionOptions {
  threadRuntime: ThreadRuntime | null;
  activeConversationId: number | null;
  activeConversation: ConversationSummary | null;
  persistedMessageIds: React.MutableRefObject<Set<string>>;
  dbIdByPosition: React.MutableRefObject<Map<number, number>>;
  syncConversations: (options?: { preferredId?: number | null; silent?: boolean }) => Promise<void>;
  showToast: (message: string, type?: ToastType, duration?: number) => void;
}

export interface UseMessageDeletionResult {
  /** Open the confirmation modal for a message. */
  initiateDelete: (runtimeMessageId: string) => void;
  /** Delete the pending message and reload the thread. */
  confirmDelete: () => Promise<void>;
  /** Dismiss the confirmation modal without deleting. */
  cancelDelete: () => void;
  /** Whether the confirmation modal is open. */
  isDeleteModalOpen: boolean;
  /** Whether a delete is in flight. */
  isDeleting: boolean;
  /** How many messages the cascade will remove, for the modal copy. */
  pendingDeleteCount: number;
}

/**
 * Message deletion with cascade, driven from the component that owns the
 * thread context.
 *
 * Deleting a message also deletes everything after it, so the thread is
 * reloaded from the database afterwards rather than patched in place —
 * `dbIdByPosition` and the persisted-ID set are rebuilt from the same rows.
 */
export function useMessageDeletion({
  threadRuntime,
  activeConversationId,
  activeConversation,
  persistedMessageIds,
  dbIdByPosition,
  syncConversations,
  showToast,
}: UseMessageDeletionOptions): UseMessageDeletionResult {
  const [deleteTargetId, setDeleteTargetId] = useState<string | null>(null);
  const [isDeleteModalOpen, setIsDeleteModalOpen] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);

  /** Count the target message plus every non-system message after it. */
  const getSubsequentMessageCount = useCallback((runtimeMessageId: string): number => {
    if (!threadRuntime) return 1;

    const state = threadRuntime.getState();
    const messageIndex = state.messages.findIndex((m) => m.id === runtimeMessageId);
    if (messageIndex === -1) return 1;

    let count = 0;
    for (let i = messageIndex; i < state.messages.length; i++) {
      if (state.messages[i].role !== 'system') {
        count++;
      }
    }
    return count;
  }, [threadRuntime]);

  const initiateDelete = useCallback((runtimeMessageId: string) => {
    setDeleteTargetId(runtimeMessageId);
    setIsDeleteModalOpen(true);
  }, []);

  const cancelDelete = useCallback(() => {
    setIsDeleteModalOpen(false);
    setDeleteTargetId(null);
  }, []);

  const confirmDelete = useCallback(async () => {
    if (!deleteTargetId || !threadRuntime || !activeConversationId) return;

    setIsDeleting(true);
    try {
      // Hydrated messages carry their DB ID in the runtime ID; messages created
      // in this session do not, so fall back to the position map.
      let dbId = extractDbId(deleteTargetId);

      if (!dbId) {
        const state = threadRuntime.getState();
        const position = state.messages.findIndex((m) => m.id === deleteTargetId);
        if (position >= 0) {
          dbId = dbIdByPosition.current.get(position) ?? null;
        }
      }

      if (dbId) {
        await getTransport().deleteMessage(dbId);
      } else {
        appLogger.debug('component.chat', 'Could not find DB ID for message', { messageId: deleteTargetId });
      }

      const dbMessages = await getTransport().getMessages(activeConversationId);
      const { messages, dbIdByPosition: positions, seededIds } = buildThreadMessages(
        dbMessages,
        activeConversation,
        activeConversationId,
      );

      dbIdByPosition.current = positions;
      persistedMessageIds.current = seededIds;
      threadRuntime.reset(messages);

      await syncConversations({ silent: true });
      showToast('Message deleted', 'success');
    } catch (error) {
      appLogger.error('component.chat', 'Failed to delete message', { error, messageId: deleteTargetId });
      showToast('Failed to delete message', 'error');
    } finally {
      setIsDeleting(false);
      setIsDeleteModalOpen(false);
      setDeleteTargetId(null);
    }
  }, [
    deleteTargetId,
    threadRuntime,
    activeConversationId,
    activeConversation,
    dbIdByPosition,
    persistedMessageIds,
    syncConversations,
    showToast,
  ]);

  return {
    initiateDelete,
    confirmDelete,
    cancelDelete,
    isDeleteModalOpen,
    isDeleting,
    pendingDeleteCount: deleteTargetId ? getSubsequentMessageCount(deleteTargetId) : 1,
  };
}
