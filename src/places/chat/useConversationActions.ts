/**
 * What changes the open conversation, from its title ▾: rename it, start it
 * over with the same prompt, export it, set its system prompt, delete it.
 * Each acts on this machine's daemon, so each does nothing on a far chat,
 * whose ids are the far machine's and could name a different chat here.
 * Every change reads the chat list again, so the sidebar says it at once.
 *
 * @module places/chat/useConversationActions
 */

import { appLogger } from '../../services/platform';
import { DEFAULT_SYSTEM_PROMPT } from '../../hooks/useGglibRuntime';
import { deleteChat, refreshChats, renameChat } from '../../services/chatRegistry';
import { getTransport } from '../../services/transport';
import type { ConversationSummary } from '../../services/transport';
import type { ModelRef } from '../../types/generated/ModelRef';
import { formatError } from '../../utils/errors';

type Confirm = (options: {
  title: string;
  description?: string;
  confirmLabel?: string;
  variant?: 'danger';
}) => Promise<boolean>;

interface ConversationActionsOptions {
  /** A far chat is open: nothing here may act. */
  far: boolean;
  activeConversation: ConversationSummary | null;
  confirm: Confirm;
  onError: (message: string) => void;
  /** Start over made a new conversation: open it. */
  onOpened: (id: number) => void;
  /** The open conversation was deleted. */
  onDeleted?: () => void;
  /** A chat with the paired machine's model: its fresh copy is made for that model too. */
  pairedModel?: ModelRef;
}

/** Ask, then delete one of this machine's chats; whether it was deleted. */
export async function confirmDeleteChat(
  confirm: Confirm,
  conversation: Pick<ConversationSummary, 'id' | 'title'>,
  onError: (message: string) => void,
): Promise<boolean> {
  const shouldDelete = await confirm({
    title: `Delete "${conversation.title}"?`,
    description: 'Its messages are deleted too. This cannot be undone.',
    confirmLabel: 'Delete',
    variant: 'danger',
  });
  if (!shouldDelete) return false;
  try {
    await deleteChat(conversation.id);
    return true;
  } catch (error) {
    onError(formatError(error));
    return false;
  }
}

export function useConversationActions({
  far,
  activeConversation,
  confirm,
  onError,
  onOpened,
  onDeleted,
  pairedModel,
}: ConversationActionsOptions) {
  const remove = async () => {
    if (far || !activeConversation) return;
    if (await confirmDeleteChat(confirm, activeConversation, onError)) onDeleted?.();
  };

  const rename = async (title: string) => {
    if (far || !activeConversation) return;
    try {
      // The open chat stays open: a title can arrive after its chat was left.
      await renameChat(activeConversation.id, title);
    } catch (error) {
      appLogger.error('component.chat', 'Rename conversation failed', { error, conversationId: activeConversation.id });
      onError(formatError(error));
    }
  };

  const startOver = async () => {
    if (far || !activeConversation) return;
    const confirmed = await confirm({
      title: 'Start over with the same prompt?',
      description: 'This conversation is deleted, and an empty one with its title and system prompt takes its place.',
      confirmLabel: 'Start over',
    });
    if (!confirmed) return;

    try {
      await getTransport().deleteConversation(activeConversation.id);
      const newId = await getTransport().createConversation({
        title: activeConversation.title,
        modelId: null,
        systemPrompt: activeConversation.system_prompt ?? DEFAULT_SYSTEM_PROMPT,
        model: pairedModel ?? null,
      });
      onOpened(newId);
    } catch (error) {
      onError(formatError(error));
    }
  };

  const exportChat = async () => {
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

  const updateSystemPrompt = async (prompt: string | null) => {
    if (far || !activeConversation) return;
    try {
      await getTransport().updateConversationSystemPrompt(activeConversation.id, prompt);
      await refreshChats('this');
    } catch (error) {
      onError(formatError(error));
    }
  };

  return { remove, rename, startOver, exportChat, updateSystemPrompt };
}
