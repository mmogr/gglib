/**
 * `/chat/:modelId`: a chat on one of this machine's models, served here.
 *
 * The port is the server registry's, or the one the start that opened this
 * chat handed on, since the registry may not have heard of it yet; with
 * neither, the model is not running, and the address goes to its inspector,
 * where Load is. Keyed by the model, so a switch remounts the page on the
 * new one. Close leaves the chat for the model's inspector: the proxy serves
 * its model to every client, Copilot included, so unloading it is a separate
 * action.
 *
 * @module app/ChatRoute
 */

import { lazy, Suspense, useEffect, useState } from 'react';
import { Navigate, useNavigate, useParams, useSearchParams } from 'react-router';

import { useServers } from '../hooks/useServers';
import { useModel } from '../services/modelRegistry';
import { clearChatHandoff, peekChatHandoff } from './chatHandoff';
import { paths } from './paths';
import { useChatSwitch } from './useChatSwitch';

// Lazy: the chat brings assistant-ui, which the library does not need.
const ChatPage = lazy(() => import('../pages/ChatPage'));

export const CHAT_LOADING = (
  <div className="flex flex-col h-full w-full overflow-hidden">
    <div className="loading-chat">Loading chat...</div>
  </div>
);

function LocalChat({ modelId, view }: { modelId: number; view: 'chat' | 'console' }) {
  const navigate = useNavigate();
  const { servers, stopServer } = useServers();
  const server = servers.find((s) => s.modelId === modelId);
  const [handoff] = useState(() => peekChatHandoff(modelId));
  useEffect(() => () => clearChatHandoff(modelId), [modelId]);
  const { startingModel, switchTo } = useChatSwitch(servers);
  const inLibrary = useModel(modelId);

  const port = server?.port ?? handoff?.port;
  if (port === undefined) return <Navigate to={paths.model(modelId)} replace />;

  return (
    <ChatPage
      serverPort={port}
      modelId={modelId}
      modelName={server?.modelName ?? handoff?.modelName ?? inLibrary?.name ?? `Model #${modelId}`}
      initialView={view}
      conversationId={handoff?.conversationId}
      draft={handoff?.draft}
      startingModel={startingModel}
      onSwitchModel={switchTo}
      onUnloadModel={() => stopServer(modelId)}
      onClose={() => navigate(paths.model(modelId))}
    />
  );
}

export function ChatRoute() {
  const { modelId: raw } = useParams();
  const [search] = useSearchParams();
  const modelId = /^\d+$/.test(raw ?? '') ? Number(raw) : null;
  if (modelId === null) return <Navigate to={paths.library()} replace />;
  const view = search.get('view') === 'console' ? 'console' : 'chat';
  return (
    <Suspense fallback={CHAT_LOADING}>
      <LocalChat key={modelId} modelId={modelId} view={view} />
    </Suspense>
  );
}
