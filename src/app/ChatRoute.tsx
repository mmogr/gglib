/**
 * `/chat/:id` and `/chat/new`: one of this machine's conversations, on the
 * model it names, or an empty chat whose first send makes one.
 *
 * The model is, in order: the one a handoff names (a model switch, or a
 * start that just served it); the conversation's own; the one this route
 * just opened the chat on (a first send, a branch, Start over, before a run
 * has named it); for an empty chat, `?far=` or `?model=`. Failing those
 * (a chat that has not run, or `/chat/new` alone): the last one used if it
 * is loaded, else the default model if it is, else the first loaded; with
 * none loaded, the inspector of the last one used (or the default), where
 * Load is, else the library.
 *
 * A chat on this machine's model needs its port: the server registry's, or
 * the handoff's, since the registry may not have heard of it yet; with
 * neither, the model is not running, and the address goes to its inspector.
 * Once open, the chat keeps the last port it had: a model unloaded or
 * crashed while it is open leaves it open, its composer saying so.
 * A chat on the paired machine's model is `PairedChat`. One route serves
 * `/chat/new` and `/chat/:id`, and the place is keyed by its model, so the
 * address a first send replaces keeps the page. Leaving keeps the model
 * loaded: the endpoint serves it to every client, Copilot included.
 *
 * @module app/ChatRoute
 */

import { lazy, Suspense, useEffect, useState } from 'react';
import { Navigate, useLocation, useNavigate, type Location } from 'react-router';

import { useServers, type ServerViewModel } from '../hooks/useServers';
import { useSettings } from '../hooks/useSettings';
import { lastUsedModel, modelOf, refreshChats, useChats, type ChatModel } from '../services/chatRegistry';
import { useModel } from '../services/modelRegistry';
import type { ConversationSummary } from '../services/transport';
import type { Machine } from '../types/generated/Machine';
import { clearChatHandoff, peekChatHandoff, wasHanded, type ChatHandoff } from './chatHandoff';
import { CHAT_LOADING, ChatUnread } from './chatLoading';
import { PairedChat } from './PairedChat';
import { chatAddress, paths } from './paths';
import { useChatSwitch } from './useChatSwitch';

// Lazy: the chat brings assistant-ui, which the library does not need.
const ChatPlace = lazy(() => import('../places/chat/ChatPlace'));

/** Where a chat opens: on a model, or (`to`) somewhere else entirely. */
type Target = { kind: 'local'; id: number } | { kind: 'paired'; id: number; machine?: Machine } | { kind: 'go'; to: string };

/**
 * The model for a chat that names none: the last used, then the default,
 * whichever is loaded; else the first loaded; else the inspector of one of
 * them, where Load is; else the library.
 */
function fallback(list: readonly ConversationSummary[], defaultId: number | null, servers: ServerViewModel[]): Target {
  const running = servers.filter((s) => s.status === 'running').map((s) => s.modelId);
  const wanted = [lastUsedModel(list), defaultId].filter((id): id is number => id !== null);
  const loaded = wanted.find((id) => running.includes(id)) ?? running[0];
  if (loaded !== undefined) return { kind: 'local', id: loaded };
  return { kind: 'go', to: wanted[0] === undefined ? paths.library() : paths.model(wanted[0]) };
}

function targetOf(model: ChatModel): Target | null {
  if (model === null) return null;
  return model.kind === 'local' ? { kind: 'local', id: model.id } : { kind: 'paired', id: model.model.id, machine: model.model.machine };
}

/**
 * The handoff the navigation to `location` set, held for as long as the
 * route stays at that location, and dropped from the store once committed.
 */
function useHandoff(location: Location): ChatHandoff | null {
  const read = () => ({ key: location.key, handoff: wasHanded(location.state) ? peekChatHandoff() : null });
  const [held, setHeld] = useState(read);
  let current = held;
  if (held.key !== location.key) {
    current = read();
    setHeld(current);
  }
  useEffect(() => clearChatHandoff(current.handoff), [current.handoff]);
  return current.handoff;
}

interface LocalChatProps {
  modelId: number;
  conversationId: number | null;
  handoff: ChatHandoff | null;
  onOpened: (id: number) => void;
  onDeleted: () => void;
}

function LocalChat({ modelId, conversationId, handoff, onOpened, onDeleted }: LocalChatProps) {
  const navigate = useNavigate();
  const { servers, stopServer } = useServers();
  const server = servers.find((s) => s.modelId === modelId);
  const { startingModel, switchTo } = useChatSwitch(servers);
  const inLibrary = useModel(modelId);
  const handed = handoff?.modelId === modelId ? handoff : null;

  // The port last served on is kept: a model unloaded or crashed mid-chat
  // keeps the chat open, its composer saying so. Only a chat opened on a
  // model nothing serves goes to its inspector, where Load is.
  const live = server?.port ?? handed?.port;
  const [held, setHeld] = useState(live);
  if (live !== undefined && live !== held) setHeld(live);
  const port = live ?? held;
  if (port === undefined) return <Navigate to={paths.model(modelId)} replace />;

  return (
    <ChatPlace
      serverPort={port}
      modelId={modelId}
      modelName={server?.modelName ?? handed?.modelName ?? inLibrary?.name ?? `Model #${modelId}`}
      conversationId={conversationId}
      draft={handed?.draft}
      startingModel={startingModel}
      onSwitchModel={switchTo}
      onUnloadModel={() => stopServer(modelId)}
      onOpened={onOpened}
      onDeleted={onDeleted}
      onShowModel={() => navigate(paths.model(modelId))}
      onShowLog={() => navigate(paths.log(modelId))}
      onOpenFar={(id) => navigate(paths.newChat({ far: id }))}
    />
  );
}

export function ChatRoute() {
  const location = useLocation();
  const navigate = useNavigate();
  const address = chatAddress(location);
  const handoff = useHandoff(location);
  const { list, failed } = useChats('this');
  const { servers } = useServers();
  const { settings } = useSettings();
  // The chat this route just opened, and on what: kept until a run names it.
  const [opened, setOpened] = useState<{ id: number; target: Target } | null>(null);
  // The location whose missing conversation the list was read again for.
  const [rechecked, setRechecked] = useState<string | null>(null);
  // The location whose unreadable list was read once more.
  const [reread, setReread] = useState<string | null>(null);

  const conversationId = address?.kind === 'saved' ? address.id : null;
  const conversation = conversationId === null ? null : list?.find((c) => c.id === conversationId) ?? null;
  const justOpened = opened !== null && opened.id === conversationId ? opened.target : null;
  // A list that could not be read says nothing of whether the chat is there,
  // nor of its model: it is read once more, and never guessed.
  const missing = conversationId !== null && list !== null && !failed && conversation === null && !handoff && !justOpened;
  const unread = conversationId !== null && failed && conversation === null && !handoff && !justOpened;

  useEffect(() => {
    if (list === null) void refreshChats('this');
  }, [list]);
  useEffect(() => {
    if (!missing || rechecked === location.key) return;
    const key = location.key;
    void refreshChats('this').then(() => setRechecked(key));
  }, [missing, rechecked, location.key]);

  useEffect(() => {
    if (!unread || reread === location.key) return;
    const key = location.key;
    void refreshChats('this').then(() => setReread(key));
  }, [unread, reread, location.key]);

  if (address === null || address.kind === 'far') return <Navigate to={paths.library()} replace />;

  let target: Target | null = handoff ? { kind: 'local', id: handoff.modelId } : null;
  if (target === null && conversation) target = targetOf(modelOf(conversation));
  if (target === null) target = justOpened;
  if (target === null && address.kind === 'new') {
    if (address.far !== null) target = { kind: 'paired', id: address.far };
    else if (address.model !== null) target = { kind: 'local', id: address.model };
  }
  if (target === null) {
    if (missing) return rechecked === location.key ? <Navigate to={paths.newChat()} replace /> : CHAT_LOADING;
    if (unread) return reread === location.key ? <ChatUnread onRetry={() => setReread(null)} /> : CHAT_LOADING;
    if (list === null) return CHAT_LOADING;
    target = fallback(list, settings?.defaultModelId ?? null, servers);
  }
  if (target.kind === 'go') return <Navigate to={target.to} replace />;

  const on = target;
  const onOpened = (id: number) => {
    setOpened({ id, target: on });
    // A run's end opens the chat it is in: already the address, which a
    // replace would make a new entry, dropping a model switch's handoff.
    if (id !== conversationId) navigate(paths.chat(id), { replace: true });
  };
  const onDeleted = () =>
    navigate(paths.newChat(on.kind === 'local' ? { model: on.id } : { far: on.id }), { replace: true });

  return (
    <Suspense fallback={CHAT_LOADING}>
      {on.kind === 'local' ? (
        <LocalChat key={`local:${on.id}`} modelId={on.id} conversationId={conversationId} handoff={handoff} onOpened={onOpened} onDeleted={onDeleted} />
      ) : (
        <PairedChat
          key={`paired:${on.id}`}
          id={on.id}
          machine={on.machine}
          conversationId={conversationId}
          onOpened={onOpened}
          onDeleted={onDeleted}
        />
      )}
    </Suspense>
  );
}
