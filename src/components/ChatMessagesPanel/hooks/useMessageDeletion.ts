import { useCallback, useState } from 'react';
import type { ThreadRuntime } from '@assistant-ui/react';
import { appLogger } from '../../../services/platform';
import { getTransport, TransportError } from '../../../services/transport';
import type { ConversationSummary } from '../../../services/transport';
import { extractDbId } from '../components/MessageActionsContext';
import { buildThreadMessages } from '../../../hooks/useChatPersistence/buildThreadMessages';
import type { ToastType } from '../../Toast';

export interface UseMessageDeletionOptions {
  threadRuntime: ThreadRuntime | null;
  activeConversationId: number | null;
  activeConversation: ConversationSummary | null;
  syncConversations: (options?: { preferredId?: number | null; silent?: boolean }) => Promise<void>;
  showToast: (message: string, type?: ToastType, duration?: number) => void;
  /**
   * A far chat: its rows are the far machine's, and a delete here would
   * reach this machine's daemon with its ids. Nothing is deleted.
   */
  readOnly?: boolean;
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
 * reloaded from the database afterwards rather than patched in place.
 *
 * Only a saved row can be deleted, and every saved row carries its id in its
 * runtime id (`db-<id>`): a reply is shown from its saved rows once its run
 * ends. A message still being drawn has none, and deleting it deletes
 * nothing.
 */
export function useMessageDeletion({
  threadRuntime,
  activeConversationId,
  activeConversation,
  syncConversations,
  showToast,
  readOnly = false,
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
    if (readOnly) return;
    setDeleteTargetId(runtimeMessageId);
    setIsDeleteModalOpen(true);
  }, [readOnly]);

  const cancelDelete = useCallback(() => {
    setIsDeleteModalOpen(false);
    setDeleteTargetId(null);
  }, []);

  const confirmDelete = useCallback(async () => {
    if (readOnly || !deleteTargetId || !threadRuntime || !activeConversationId) return;

    setIsDeleting(true);
    try {
      const dbId = extractDbId(deleteTargetId);
      if (dbId) {
        await getTransport().deleteMessage(dbId);
      } else {
        appLogger.debug('component.chat', 'Could not find DB ID for message', { messageId: deleteTargetId });
      }

      const dbMessages = (await getTransport().getThread(activeConversationId)).messages;
      threadRuntime.reset(buildThreadMessages(dbMessages, activeConversation, activeConversationId));

      await syncConversations({ silent: true });
      showToast('Message deleted', 'success');
    } catch (error) {
      appLogger.error('component.chat', 'Failed to delete message', { error, messageId: deleteTargetId });
      // The daemon refuses while a reply to the chat is still being written.
      showToast(
        TransportError.hasCode(error, 'CONFLICT')
          ? 'A reply is still being written here. Stop it or wait, then delete.'
          : 'Failed to delete message',
        'error',
      );
    } finally {
      setIsDeleting(false);
      setIsDeleteModalOpen(false);
      setDeleteTargetId(null);
    }
  }, [
    readOnly,
    deleteTargetId,
    threadRuntime,
    activeConversationId,
    activeConversation,
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
