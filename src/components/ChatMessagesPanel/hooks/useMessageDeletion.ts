import { useCallback, useEffect, useRef } from 'react';
import type { ThreadRuntime } from '@assistant-ui/react';
import { appLogger } from '../../../services/platform';
import { getTransport, TransportError } from '../../../services/transport';
import type { ConversationSummary } from '../../../services/transport';
import type { ConfirmOptions } from '../../ui/ConfirmDialog';
import { extractDbId } from '../components/MessageActionsContext';
import { buildThreadMessages } from '../../../hooks/useChatPersistence/buildThreadMessages';
import type { ToastType } from '../../Toast';

export interface UseMessageDeletionOptions {
  threadRuntime: ThreadRuntime | null;
  activeConversationId: number | null;
  activeConversation: ConversationSummary | null;
  syncConversations: (options?: { preferredId?: number | null; silent?: boolean }) => Promise<void>;
  showToast: (message: string, type?: ToastType, duration?: number) => void;
  /** The one confirm sheet's question (`app/confirm`), answered by the person. */
  confirm: (opts: ConfirmOptions) => Promise<boolean>;
  /**
   * A far chat: its rows are the far machine's, and a delete here would
   * reach this machine's daemon with its ids. Nothing is deleted.
   */
  readOnly?: boolean;
}

/** What the confirm sheet says of deleting a message and the `count - 1` after it. */
export function deleteQuestion(count: number): ConfirmOptions {
  const after = count - 1;
  return {
    title: 'Delete this message?',
    description:
      after > 0
        ? `The ${after} ${after === 1 ? 'message' : 'messages'} after it ${after === 1 ? 'is' : 'are'} deleted too. This cannot be undone.`
        : 'This cannot be undone.',
    confirmLabel: 'Delete',
    variant: 'danger',
  };
}

/**
 * Message deletion with cascade, driven from the component that owns the
 * thread context: Delete… asks through the one confirm sheet, saying how
 * many messages after it go too, then deletes.
 *
 * Deleting a message also deletes everything after it, so the thread is
 * reloaded from the database afterwards rather than patched in place.
 *
 * Only a saved row can be deleted, and every saved row carries its id in its
 * runtime id (`db-<id>`): a reply is shown from its saved rows once its run
 * ends. A message still being drawn has none, and deleting it deletes
 * nothing. The daemon refuses (409) while a reply to the chat is being
 * written, and the toast says so.
 */
export function useMessageDeletion({
  threadRuntime,
  activeConversationId,
  activeConversation,
  syncConversations,
  showToast,
  confirm,
  readOnly = false,
}: UseMessageDeletionOptions): (runtimeMessageId: string) => Promise<void> {
  // Read again once the question is answered: the page may have turned to a far chat meanwhile.
  const readOnlyNow = useRef(readOnly);
  useEffect(() => {
    readOnlyNow.current = readOnly;
  }, [readOnly]);

  /** The target message plus every non-system message after it. */
  const cascadeCount = useCallback((runtimeMessageId: string): number => {
    const messages = threadRuntime?.getState().messages ?? [];
    const at = messages.findIndex((m) => m.id === runtimeMessageId);
    if (at === -1) return 1;
    return messages.slice(at).filter((m) => m.role !== 'system').length;
  }, [threadRuntime]);

  return useCallback(async (runtimeMessageId: string) => {
    if (readOnly || !threadRuntime || !activeConversationId) return;
    if (!(await confirm(deleteQuestion(cascadeCount(runtimeMessageId))))) return;
    if (readOnlyNow.current) return;

    try {
      const dbId = extractDbId(runtimeMessageId);
      if (dbId) {
        await getTransport().deleteMessage(dbId);
      } else {
        appLogger.debug('component.chat', 'Could not find DB ID for message', { messageId: runtimeMessageId });
      }

      const dbMessages = (await getTransport().getThread(activeConversationId)).messages;
      threadRuntime.reset(buildThreadMessages(dbMessages, activeConversation, activeConversationId));

      await syncConversations({ silent: true });
      showToast('Message deleted', 'success');
    } catch (error) {
      appLogger.error('component.chat', 'Failed to delete message', { error, messageId: runtimeMessageId });
      // The daemon refuses while a reply to the chat is still being written.
      showToast(
        TransportError.hasCode(error, 'CONFLICT')
          ? 'A reply is still being written here. Stop it or wait, then delete.'
          : 'Failed to delete message',
        'error',
      );
    }
  }, [readOnly, threadRuntime, activeConversationId, activeConversation, syncConversations, showToast, confirm, cascadeCount]);
}
