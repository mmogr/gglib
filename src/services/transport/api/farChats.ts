/**
 * The far machine's chats and runs, at `/api/remote/*`: this machine's
 * daemon forwards each request through the tunnel with the key it holds, so
 * the page never does. What comes back is shown, never kept.
 *
 * A turn sends only its text: the far machine rebuilds the history from its
 * own record, runs the reply and saves it there.
 */

import type { HubChat } from '../../../types/generated/HubChat';
import type { HubChatList } from '../../../types/generated/HubChatList';
import type { HubChatOpen } from '../../../types/generated/HubChatOpen';
import type { RemoteTurnBody } from '../../../types/generated/RemoteTurnBody';
import type { RunInfo } from '../../../types/generated/RunInfo';
import type { RunList } from '../../../types/generated/RunList';
import { REMOTE_CHATS_PATH, REMOTE_RUNS_PATH } from '../../api/routes';
import { get, post, put } from './client';
import { readRunStream, type RunStreamItem } from './runs';

function farRunPath(id: string): string {
  return `${REMOTE_RUNS_PATH}/${encodeURIComponent(id)}`;
}

/** The far machine's chats, newest first. */
export async function listFarChats(): Promise<HubChat[]> {
  return (await get<HubChatList>(REMOTE_CHATS_PATH)).chats;
}

/** One far chat and its rows, oldest first. */
export async function openFarChat(id: number): Promise<HubChatOpen> {
  return get<HubChatOpen>(`${REMOTE_CHATS_PATH}/${id}`);
}

/** Add `content` to far chat `id` as run `runId`; the far machine runs the reply. */
export async function addFarTurn(id: number, runId: string, content: string): Promise<RunInfo> {
  const body: RemoteTurnBody = { content };
  return put<RunInfo>(`${REMOTE_CHATS_PATH}/${id}/turns/${encodeURIComponent(runId)}`, body);
}

/** The far runs this machine may see, newest first. */
export async function listFarRuns(): Promise<RunInfo[]> {
  return (await get<RunList>(REMOTE_RUNS_PATH)).runs;
}

/** Cancel a far run. */
export async function cancelFarRun(id: string): Promise<RunInfo> {
  return post<RunInfo>(`${farRunPath(id)}/cancel`);
}

/** A far run's events after `after`, then its final state. */
export function readFarRunEvents(
  id: string,
  after: number,
  signal: AbortSignal,
): AsyncGenerator<RunStreamItem> {
  return readRunStream(`${farRunPath(id)}/events?after=${after}`, signal);
}
