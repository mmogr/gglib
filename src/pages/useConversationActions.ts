/**
 * What changes a conversation on the chat page: delete it, rename it, start
 * it afresh, export it, set its system prompt. Each acts on this machine's
 * daemon, so each does nothing on a far chat, whose ids are the far
 * machine's and could name a different chat here.
 *
 * @module useConversationActions
 */

import { appLogger } from '../services/platform';
import { DEFAULT_SYSTEM_PROMPT } from '../hooks/useGglibRuntime';
import { getTransport } from '../services/transport';
import type { ConversationSummary } from '../services/transport';
import { formatError } from '../utils/errors';

interface ConversationActionsOptions {
  /** A far chat is open: nothing here may act. */
  far: boolean;
  activeConversation: ConversationSummary | null;
  confirm: (options: {
    title: string;
    description?: string;
    confirmLabel?: string;
    variant?: 'danger';
  }) => Promise<boolean>;
  syncConversations: (options?: { preferredId?: number | null; silent?: boolean }) => Promise<void>;
  onError: (message: string) => void;
}

export function useConversationActions({
  far,
  activeConversation,
  confirm,
  syncConversations,
  onError,
}: ConversationActionsOptions) {
  const handleDeleteConversation = async (conversationId: number) => {
    if (far) return;
    const shouldDelete = await confirm({
      title: 'Delete this conversation?',
      description: 'This cannot be undone.',
      confirmLabel: 'Delete',
      variant: 'danger',
    });
    if (!shouldDelete) return;

    try {
      await getTransport().deleteConversation(conversationId);
      await syncConversations();
    } catch (error) {
      onError(formatError(error));
    }
  };

  const handleRenameConversation = async (title: string) => {
    if (far || !activeConversation) return;
    try {
      appLogger.debug('component.chat', 'Rename conversation called', {
        conversationId: activeConversation.id,
        title,
        titleLength: title.length,
      });
      await getTransport().updateConversationTitle(activeConversation.id, title);
      appLogger.debug('component.chat', 'Title update succeeded, syncing');
      // The open chat stays open: a title can arrive after its chat was left.
      await syncConversations({ silent: true });
      appLogger.debug('component.chat', 'Rename conversation completed successfully');
    } catch (error) {
      appLogger.error('component.chat', 'Rename conversation failed', {
        error,
        conversationId: activeConversation.id,
        title
      });
      onError(formatError(error));
    }
  };

  const handleClearConversation = async () => {
    if (far || !activeConversation) return;
    const confirmed = await confirm({
      title: 'Start a fresh copy?',
      description: 'The current conversation will be deleted and replaced with a new copy.',
      confirmLabel: 'Start fresh',
    });
    if (!confirmed) return;

    try {
      await getTransport().deleteConversation(activeConversation.id);
      const newId = await getTransport().createConversation({
        title: activeConversation.title,
        modelId: null,
        systemPrompt: activeConversation.system_prompt ?? DEFAULT_SYSTEM_PROMPT,
      });
      await syncConversations({ preferredId: newId });
    } catch (error) {
      onError(formatError(error));
    }
  };

  const handleExportConversation = async () => {
    if (far || !activeConversation) return;
    try {
      const { messages } = await getTransport().getThread(activeConversation.id);
      const data = { conversation: activeConversation, messages };
      const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement('a');
      anchor.href = url;
      anchor.download = `conversation-${activeConversation.id}.json`;
      anchor.click();
      URL.revokeObjectURL(url);
    } catch (error) {
      onError(formatError(error));
    }
  };

  const handleUpdateSystemPrompt = async (prompt: string | null) => {
    if (far || !activeConversation) return;
    try {
      await getTransport().updateConversationSystemPrompt(activeConversation.id, prompt);
      await syncConversations({ preferredId: activeConversation.id, silent: true });
    } catch (error) {
      onError(formatError(error));
    }
  };

  return {
    handleDeleteConversation,
    handleRenameConversation,
    handleClearConversation,
    handleExportConversation,
    handleUpdateSystemPrompt,
  };
}
