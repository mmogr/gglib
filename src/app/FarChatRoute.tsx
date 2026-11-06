/**
 * `/chat/far/:id`: a chat on the paired machine's model `id` (its id there).
 *
 * The machine is fixed for the chat's life: the model and the machine's name
 * are read from the paired rows when the chat opens, kept across that
 * machine's disconnection and a new connection that names no machine yet,
 * and the chat is closed once the status names another machine, whose same
 * id is another model (`stillPaired`). With no rows yet and a machine
 * connected, the chat waits for them; with none connected, there is nothing
 * to chat with and the address goes to the library.
 *
 * @module app/FarChatRoute
 */

import { lazy, Suspense, useState } from 'react';
import { Navigate, useNavigate, useParams } from 'react-router';

import { usePairedModels } from '../hooks/usePairedModels';
import { stillPaired, useRemoteState } from '../services/remoteRegistry';
import type { ModelRef } from '../types/generated/ModelRef';
import { CHAT_LOADING } from './ChatRoute';
import { paths } from './paths';

const ChatPage = lazy(() => import('../pages/ChatPage'));

interface FarChat {
  far: ModelRef;
  modelName: string;
  machineName: string;
}

function OpenedFarChat({ id }: { id: number }) {
  const navigate = useNavigate();
  const paired = usePairedModels();
  const { status } = useRemoteState();
  const [opened, setOpened] = useState<FarChat | null>(null);

  if (opened === null) {
    const group = paired.group;
    const model = group?.models.find((m) => m.gglib_id === id);
    if (group && model) {
      // `id` on the wire is what a client sends, the model's name here.
      setOpened({ far: { machine: group.machine, id }, modelName: model.id, machineName: paired.name });
      return null;
    }
    if (status?.connected) return CHAT_LOADING;
    return <Navigate to={paths.library()} replace />;
  }

  if (!stillPaired(opened.far.machine, status)) return <Navigate to={paths.library()} replace />;

  return (
    <ChatPage
      paired={{ far: opened.far, machineName: opened.machineName }}
      modelName={opened.modelName}
      onClose={() => navigate(paths.far(id))}
    />
  );
}

export function FarChatRoute() {
  const { id: raw } = useParams();
  const id = /^\d+$/.test(raw ?? '') ? Number(raw) : null;
  if (id === null) return <Navigate to={paths.library()} replace />;
  return (
    <Suspense fallback={CHAT_LOADING}>
      <OpenedFarChat key={id} id={id} />
    </Suspense>
  );
}
