import { useMemo } from 'react';
import type { ThreadRuntime } from '@assistant-ui/react';
import { getTransport, type ChatSource, type ConversationSummary } from '../../../services/transport';
import { buildThreadMessages } from '../../../hooks/useChatPersistence/buildThreadMessages';
import type { ImageModelRow } from '../../../types/generated/ImageModelRow';
import type { PictureActions } from '../components/MessageActionsContext';

export interface UsePictureKeepingOptions {
  threadRuntime: ThreadRuntime | null;
  activeConversationId: number | null;
  activeConversation: ConversationSummary | null;
  source: ChatSource;
  /** This machine's image models, as the Draw button's answer lists them. */
  models: readonly ImageModelRow[] | undefined;
  /** The image model the chat has chosen, as the Draw menu holds it. */
  picked: number | null | undefined;
}

/**
 * What the open chat's drawn pictures are fixed and redone with, or nothing
 * where they cannot be: a far chat, or one not saved yet. Once a newer
 * picture is kept the chat is read again and shown, as after a deletion,
 * unless a reply is being written: then the picture shows when the chat is
 * next read, and the reply is left alone.
 */
export function usePictureKeeping({
  threadRuntime,
  activeConversationId,
  activeConversation,
  source,
  models,
  picked,
}: UsePictureKeepingOptions): PictureActions | undefined {
  return useMemo(() => {
    if (source !== 'this' || !threadRuntime || activeConversationId === null) return undefined;
    return {
      conversationId: activeConversationId,
      models: models ?? [],
      picked: picked ?? null,
      onKept: async () => {
        if (threadRuntime.getState().isRunning) return;
        const rows = (await getTransport().getThread(activeConversationId)).messages;
        threadRuntime.reset(buildThreadMessages(rows, activeConversation, activeConversationId));
      },
    };
  }, [source, threadRuntime, activeConversationId, activeConversation, models, picked]);
}
