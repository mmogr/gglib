/**
 * Which chat screen the Model Control Center is showing, and why.
 *
 * There are two kinds and they are not variations of one shape. A local
 * session is a model served on this machine: it has a port, a row in the
 * server registry, and a Console tab reading that server's log. A remote one
 * is the machine on the other end of the tunnel, named only by the string
 * typed into the Remote panel — no port here, no model id here, and nothing
 * local whose health could be reported. Modelling them as one record with
 * optional fields is what let the page silently do nothing when the fields
 * were absent, so they are a union and the page has to say which it means.
 *
 * The remote request arrives through `remoteRegistry` rather than a prop:
 * the Remote panel is mounted inside the model library's header, and this
 * page replaces that whole tree with the chat screen when a session opens,
 * so the two are never in scope together.
 */

import { useCallback, useEffect, useState } from 'react';

import type { ServerViewModel } from '../../hooks/useServers';
import { clearRemoteChatRequest, useRemoteState } from '../../services/remoteRegistry';

/**
 * An open chat screen.
 *
 * `kind` is the discriminant every consumer must branch on: stopping a
 * server, reading its console and subscribing to its health are all local-only
 * operations, and the remote arm simply does not carry what they need.
 */
export type ChatSession =
  | {
      kind: 'local';
      serverPort: number;
      modelId: number;
      modelName: string;
      initialView: 'chat' | 'console';
    }
  | { kind: 'remote'; modelName: string };

export interface UseChatSessionResult {
  chatSession: ChatSession | null;
  setChatSession: (session: ChatSession | null) => void;
  /** Open the chat screen on a model already served here. */
  openChatSession: (modelId: number, view: 'chat' | 'console') => void;
  closeChatSession: () => void;
}

export function useChatSession(servers: ServerViewModel[]): UseChatSessionResult {
  const [chatSession, setChatSession] = useState<ChatSession | null>(null);
  const { chatRequestedAt, chatModel } = useRemoteState();

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

  const closeChatSession = useCallback(() => setChatSession(null), []);

  // The Remote panel asked for the far machine. Cleared as it is served so
  // the next ask is a new value; the model name is taken as typed, which is
  // the only name the far machine answers to.
  useEffect(() => {
    if (chatRequestedAt === null) return;
    clearRemoteChatRequest();
    setChatSession({ kind: 'remote', modelName: chatModel.trim() });
  }, [chatRequestedAt, chatModel]);

  return { chatSession, setChatSession, openChatSession, closeChatSession };
}
