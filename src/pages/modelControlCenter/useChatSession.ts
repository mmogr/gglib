/**
 * Which chat screen the Model Control Center is showing, and why.
 *
 * There are two kinds and they are not variations of one shape. A local
 * session is a model served on this machine: it has a port, a row in the
 * server registry, and a Console tab reading that server's log. A paired one
 * is a model of the machine on the other end of the tunnel, named by that
 * machine and its id there — no port here, no model id of this machine's,
 * and nothing local whose health could be reported. Modelling them as one
 * record with optional fields is what let the page silently do nothing when
 * the fields were absent, so they are a union and the page has to say which
 * it means.
 *
 * A paired session is opened from the far model's inspector, and its machine
 * is fixed for its life: it is closed, not moved, once the status names
 * another machine (`stillPaired`).
 */

import { useCallback, useEffect, useRef, useState } from 'react';

import type { ServerViewModel } from '../../hooks/useServers';
import type { ModelChoice } from '../../components/ChatMessagesPanel';
import { stillPaired, useRemoteState } from '../../services/remoteRegistry';
import { getTransport } from '../../services/transport';
import type { ModelRef } from '../../types/generated/ModelRef';

/**
 * An open chat screen.
 *
 * `kind` is the discriminant every consumer must branch on: stopping a
 * server, reading its console and subscribing to its health are all local-only
 * operations, and the paired arm simply does not carry what they need.
 */
export type ChatSession =
  | {
      kind: 'local';
      serverPort: number;
      modelId: number;
      modelName: string;
      initialView: 'chat' | 'console';
      /** The conversation to open with: the one open before a model switch. */
      conversationId?: number | null;
      /** The unsent text to put back in the composer after a model switch. */
      draft?: string;
    }
  | {
      kind: 'paired';
      /** The model, by the paired machine and its id there. */
      far: ModelRef;
      /** Its name there. */
      modelName: string;
      /** The name that machine is shown by. */
      machineName: string;
    };

/** What a model switch carries to the new page, read when the switch lands. */
export interface SwitchContext {
  conversationId: number | null;
  /** The composer's unsent text, put back in the new page's composer. */
  draft: string;
}

export interface UseChatSessionResult {
  chatSession: ChatSession | null;
  setChatSession: (session: ChatSession | null) => void;
  /** Open the chat screen on a model already served here. */
  openChatSession: (modelId: number, view: 'chat' | 'console') => void;
  /** Open the chat screen on a model of the paired machine. */
  openPairedChat: (far: ModelRef, modelName: string, machineName: string) => void;
  /**
   * Move the open chat `from` to another model, keeping its conversation
   * open. Lands only if it is the newest switch and `from` is still the open
   * chat when the model is up.
   */
  switchChatModel: (from: ChatSession, choice: ModelChoice, context: () => SwitchContext) => Promise<void>;
  /** The model the open chat is being moved to while it starts, if any. */
  startingModel: string | null;
  closeChatSession: () => void;
}

export function useChatSession(servers: ServerViewModel[]): UseChatSessionResult {
  const [chatSession, setChatSession] = useState<ChatSession | null>(null);
  const { status } = useRemoteState();
  // The newest switch, and the model it is starting for which chat. Held
  // here rather than in the picker, which remounts with each conversation.
  const latestSwitch = useRef(0);
  const [starting, setStarting] = useState<{ from: ChatSession; name: string } | null>(null);

  const openChatSession = useCallback(
    (modelId: number, view: 'chat' | 'console') => {
      const server = servers.find((s) => s.modelId === modelId);
      if (server) {
        setChatSession({
          kind: 'local',
          serverPort: server.port,
          modelId: server.modelId,
          modelName: server.modelName,
          initialView: view,
        });
      }
    },
    [servers],
  );

  // A model that is not running is served first with an empty request, so
  // the daemon launches it on the model's saved settings and its own
  // defaults. The server the chat leaves is left running.
  //
  // Starting a model takes long enough for the chat to be closed, reopened
  // or moved again meanwhile; that later choice wins and this switch is
  // dropped. The conversation is read when the switch lands, not when the
  // model was picked, so one chosen while it loaded is the one that opens.
  const switchChatModel = useCallback(
    async (from: ChatSession, choice: ModelChoice, context: () => SwitchContext) => {
      const ticket = ++latestSwitch.current;
      const server = servers.find((s) => s.modelId === choice.modelId);
      let port = server?.port;
      if (port === undefined) {
        setStarting({ from, name: choice.modelName });
        try {
          port = (await getTransport().serveModel({ id: choice.modelId })).port;
        } finally {
          if (ticket === latestSwitch.current) setStarting(null);
        }
      }
      if (ticket !== latestSwitch.current) return;
      const serverPort = port;
      const { conversationId, draft } = context();
      setChatSession((current) =>
        current !== from
          ? current
          : {
              kind: 'local',
              serverPort,
              modelId: choice.modelId,
              modelName: server?.modelName ?? choice.modelName,
              initialView: 'chat',
              conversationId,
              draft,
            },
      );
    },
    [servers],
  );

  const closeChatSession = useCallback(() => setChatSession(null), []);

  const openPairedChat = useCallback(
    (far: ModelRef, modelName: string, machineName: string) =>
      setChatSession({ kind: 'paired', far, modelName, machineName }),
    [],
  );

  // Another machine answering where this one's model was: its id there names
  // another model, so the chat closes rather than send to it.
  const orphaned = chatSession?.kind === 'paired' && !stillPaired(chatSession.far.machine, status);
  useEffect(() => {
    if (orphaned) setChatSession(null);
  }, [orphaned]);

  const startingModel = starting && starting.from === chatSession ? starting.name : null;

  return {
    chatSession: orphaned ? null : chatSession,
    setChatSession,
    openChatSession,
    openPairedChat,
    switchChatModel,
    startingModel,
    closeChatSession,
  };
}
