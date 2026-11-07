/**
 * `/chat/far/:id`: one of the paired machine's own chats (its id there),
 * which that machine runs and keeps. It is read through this machine's
 * daemon and carried on here, never changed: no rename, prompt or delete.
 * It needs no port and no model here: the far machine picks the model. With
 * no machine connected there is nothing to read, and the address goes to
 * the library.
 *
 * @module app/FarChatRoute
 */

import { lazy, Suspense } from 'react';
import { Navigate, useNavigate, useParams } from 'react-router';

import { pairedName, useRemoteState } from '../services/remoteRegistry';
import { CHAT_LOADING } from './chatLoading';
import { paths } from './paths';

const ChatPlace = lazy(() => import('../places/chat/ChatPlace'));

export function FarChatRoute() {
  const navigate = useNavigate();
  const { status } = useRemoteState();
  const { id: raw } = useParams();
  const id = /^\d+$/.test(raw ?? '') ? Number(raw) : null;
  if (id === null || !status?.connected) return <Navigate to={paths.library()} replace />;
  return (
    <Suspense fallback={CHAT_LOADING}>
      <ChatPlace
        key={id}
        desk={{ machineName: pairedName(status) }}
        modelName=""
        conversationId={id}
        onOpened={(next) => navigate(paths.farChat(next), { replace: true })}
      />
    </Suspense>
  );
}
