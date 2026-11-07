/**
 * A chat kept here on the paired machine's model `id` (its id there): the
 * conversation its address names, or an empty one (`/chat/new?far=`).
 *
 * The machine is fixed for the chat's life: the model and the machine's name
 * are read from the paired rows when the chat opens, kept across that
 * machine's disconnection and a new connection that names no machine yet,
 * and the chat is closed once the status names another machine, whose same
 * id is another model (`stillPaired`). A conversation made for one machine
 * does not open on another. With no rows yet and a machine connected, the
 * chat waits for them; with none connected, there is nothing to chat with
 * and the address goes to the library.
 *
 * @module app/PairedChat
 */

import { lazy, useState } from 'react';
import { Navigate } from 'react-router';

import { usePairedModels } from '../hooks/usePairedModels';
import { stillPaired, useRemoteState } from '../services/remoteRegistry';
import type { Machine } from '../types/generated/Machine';
import type { ModelRef } from '../types/generated/ModelRef';
import { CHAT_LOADING } from './chatLoading';
import { paths } from './paths';

const ChatPlace = lazy(() => import('../places/chat/ChatPlace'));

interface Opened {
  far: ModelRef;
  modelName: string;
  machineName: string;
}

interface PairedChatProps {
  /** The model's id on the paired machine. */
  id: number;
  /** The machine a saved conversation was made for; absent for an empty chat. */
  machine?: Machine;
  conversationId: number | null;
  onOpened: (id: number) => void;
  onDeleted: () => void;
}

export function PairedChat({ id, machine, conversationId, onOpened, onDeleted }: PairedChatProps) {
  const paired = usePairedModels();
  const { status } = useRemoteState();
  const [opened, setOpened] = useState<Opened | null>(null);

  if (opened === null) {
    if (machine && !stillPaired(machine, status)) return <Navigate to={paths.library()} replace />;
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
    <ChatPlace
      paired={{ far: opened.far, machineName: opened.machineName }}
      modelName={opened.modelName}
      conversationId={conversationId}
      onOpened={onOpened}
      onDeleted={onDeleted}
    />
  );
}
