import { useCallback, useEffect, useRef } from 'react';
import type { ThreadRuntime } from '@assistant-ui/react';
import { appLogger } from '../../../services/platform';
import { getTransport, TransportError } from '../../../services/transport';
import type { ConversationSummary } from '../../../services/transport';
import type { ConfirmOptions } from '../../ui/ConfirmDialog';
import { dismissConfirm } from '../../../app/confirm';
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
 *
 * The question belongs to the chat it was asked in. The confirm store is
 * the app's, and outlives this panel: leaving the chat while it is open
 * (⌘N, the native menu's Show Chat) answers it no, and an answer that
 * comes after the chat was left, or turned far, deletes nothing. One delete
 * at a time: Delete… asks nothing while one is asked or under way.
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
  // Read again once the question is answered: the page may have left the
  // chat, or turned to a far chat, meanwhile.
  const now = useRef({ alive: true, readOnly, conversationId: activeConversationId });
  useEffect(() => {
    now.current.readOnly = readOnly;
    now.current.conversationId = activeConversationId;
  }, [readOnly, activeConversationId]);
  useEffect(() => {
    const state = now.current;
    state.alive = true;
    return () => {
      state.alive = false;
    };
  }, []);

  /** A question this hook asked is open. */
  const asking = useRef(false);
  /** A delete is asked or under way. */
  const busy = useRef(false);
  // Leaving the chat answers its open question no.
  useEffect(() => () => {
    if (asking.current) dismissConfirm();
  }, [activeConversationId]);

  /** The target message plus every non-system message after it. */
  const cascadeCount = useCallback((runtimeMessageId: string): number => {
    const messages = threadRuntime?.getState().messages ?? [];
    const at = messages.findIndex((m) => m.id === runtimeMessageId);
    if (at === -1) return 1;
    return messages.slice(at).filter((m) => m.role !== 'system').length;
  }, [threadRuntime]);

  /** Delete the row (and every one after it), then show the chat as the daemon now has it. */
  const deleteAndReload = useCallback(async (runtimeMessageId: string, conversationId: number) => {
    try {
      const dbId = extractDbId(runtimeMessageId);
      if (dbId) {
        await getTransport().deleteMessage(dbId);
      } else {
        appLogger.debug('component.chat', 'Could not find DB ID for message', { messageId: runtimeMessageId });
      }

      const dbMessages = (await getTransport().getThread(conversationId)).messages;
      threadRuntime?.reset(buildThreadMessages(dbMessages, activeConversation, conversationId));

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
  }, [threadRuntime, activeConversation, syncConversations, showToast]);

  return useCallback(async (runtimeMessageId: string) => {
    if (readOnly || !threadRuntime || !activeConversationId || busy.current) return;
    busy.current = true;
    try {
      asking.current = true;
      let yes: boolean;
      try {
        yes = await confirm(deleteQuestion(cascadeCount(runtimeMessageId)));
      } finally {
        asking.current = false;
      }
      const still = now.current;
      if (!yes || !still.alive || still.readOnly || still.conversationId !== activeConversationId) return;
      await deleteAndReload(runtimeMessageId, activeConversationId);
    } finally {
      busy.current = false;
    }
  }, [readOnly, threadRuntime, activeConversationId, confirm, cascadeCount, deleteAndReload]);
}

