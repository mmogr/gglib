/**
 * Moving an open chat to another of this machine's models.
 *
 * The model is saved as the conversation's (`PUT /api/conversations/{id}`,
 * its settings' `model`), which is what its next turn runs on; nothing is
 * started here. A model that is not running is loaded by that turn, which
 * says so while it waits. An empty chat has no conversation yet: its
 * address names the model, and its first send makes a conversation that
 * keeps it. A later pick wins over one still being saved, and a chat that
 * has been left (this hook unmounted) hands nothing on. The conversation
 * and the draft are read when the pick is made, so the page opens at the
 * conversation's own address, on the new model, with the unsent text.
 *
 * @module app/useChatSwitch
 */

import { useCallback, useEffect, useRef } from 'react';
import { useNavigate } from 'react-router';

import type { ModelChoice } from '../components/ChatMessagesPanel';
import { refreshChats } from '../services/chatRegistry';
import { getTransport } from '../services/transport';
import type { ChatDraft } from '../types/messages';
import { HANDED, setChatHandoff } from './chatHandoff';
import { paths } from './paths';

/** What a switch carries to the new page, read when the pick is made. */
export interface SwitchContext {
  conversationId: number | null;
  draft: ChatDraft;
}

export interface ChatSwitch {
  /** Save `choice` as the chat's model and open the chat on it. */
  switchTo: (choice: ModelChoice, context: () => SwitchContext) => Promise<void>;
}

export function useChatSwitch(): ChatSwitch {
  const navigate = useNavigate();
  const latest = useRef(0);
  const open = useRef(true);

  useEffect(() => {
    open.current = true;
    return () => {
      open.current = false;
    };
  }, []);

  const switchTo = useCallback(
    async (choice: ModelChoice, context: () => SwitchContext) => {
      const ticket = ++latest.current;
      const { conversationId, draft } = context();
      if (conversationId !== null) {
        const model = { machine: { kind: 'local' as const }, id: choice.modelId };
        await getTransport().updateConversationSettings(conversationId, { model });
        void refreshChats('this');
      }
      if (ticket !== latest.current || !open.current) return;
      setChatHandoff({ modelId: choice.modelId, modelName: choice.modelName, draft });
      // The conversation keeps its address; an empty chat's names the model.
      const to = conversationId === null ? paths.newChat({ model: choice.modelId }) : paths.chat(conversationId);
      navigate(to, { state: HANDED });
    },
    [navigate],
  );

  return { switchTo };
}
