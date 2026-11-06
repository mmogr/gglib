/**
 * Moving an open chat to another of this machine's models.
 *
 * A model that is not running is served first with an empty request, so the
 * daemon launches it on the model's saved settings and its own defaults; the
 * server the chat leaves is left running. Starting a model takes long enough
 * for the chat to be closed, reopened or moved again meanwhile: a later pick
 * wins and this switch is dropped, and a chat that has been left (this hook
 * unmounted) hands nothing on. The conversation and the draft are read when
 * the switch lands, not when the model was picked, so one chosen while it
 * loaded is the one that opens.
 *
 * @module app/useChatSwitch
 */

import { useCallback, useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router';

import type { ModelChoice } from '../components/ChatMessagesPanel';
import type { ServerViewModel } from '../hooks/useServers';
import { getTransport } from '../services/transport';
import type { ChatDraft } from '../types/messages';
import { HANDED, setChatHandoff } from './chatHandoff';
import { paths } from './paths';

/** What a switch carries to the new page, read when the switch lands. */
export interface SwitchContext {
  conversationId: number | null;
  draft: ChatDraft;
}

export interface ChatSwitch {
  /** The model a switch is starting, which locks the picker, or `null`. */
  startingModel: string | null;
  switchTo: (choice: ModelChoice, context: () => SwitchContext) => Promise<void>;
}

export function useChatSwitch(servers: ServerViewModel[]): ChatSwitch {
  const navigate = useNavigate();
  const latest = useRef(0);
  const open = useRef(true);
  const [startingModel, setStartingModel] = useState<string | null>(null);

  useEffect(() => {
    open.current = true;
    return () => {
      open.current = false;
    };
  }, []);

  const switchTo = useCallback(
    async (choice: ModelChoice, context: () => SwitchContext) => {
      const ticket = ++latest.current;
      let port = servers.find((s) => s.modelId === choice.modelId)?.port;
      if (port === undefined) {
        setStartingModel(choice.modelName);
        try {
          port = (await getTransport().serveModel({ id: choice.modelId })).port;
        } finally {
          if (ticket === latest.current && open.current) setStartingModel(null);
        }
      }
      if (ticket !== latest.current || !open.current) return;
      const { conversationId, draft } = context();
      setChatHandoff({ modelId: choice.modelId, port, modelName: choice.modelName, conversationId, draft });
      navigate(paths.chat(choice.modelId), { state: HANDED });
    },
    [servers, navigate],
  );

  return { startingModel, switchTo };
}
