/**
 * The far machine's chats and runs, at `/api/remote/*`: this machine's
 * daemon forwards each request through the tunnel with the key it holds, so
 * the page never does. What comes back is shown, never kept.
 *
 * A turn sends only its text and its images, by the ids the far machine's
 * store answered (`uploadAttachment(source 'far', …)`): the far machine
 * rebuilds the history from its own record, runs the reply and saves it
 * there. The turn that changes the chat's Thinking choice says that too,
 * and the far machine remembers it.
 */

import type { HubChat } from '../../../types/generated/HubChat';
import type { HubChatList } from '../../../types/generated/HubChatList';
import type { HubChatOpen } from '../../../types/generated/HubChatOpen';
import type { RemoteTurnBody } from '../../../types/generated/RemoteTurnBody';
import type { RunInfo } from '../../../types/generated/RunInfo';
import type { RunList } from '../../../types/generated/RunList';
import type { Thinking } from '../../../types/generated/Thinking';
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

/**
 * Add `content` and `images` (ids in the far machine's store) to far chat
 * `id` as run `runId`; the far machine runs the reply. A turn with no image
 * leaves `images` out, as a far gglib from before images reads it. Only the
 * turn that changes the chat's Thinking choice carries `thinking`: a far
 * gglib from before the choice refuses the key, and lists no model as one
 * that thinks, so the page offers no switch to change it by.
 */
export async function addFarTurn(
  id: number,
  runId: string,
  content: string,
  images: string[] = [],
  thinking?: Thinking,
): Promise<RunInfo> {
  const body: RemoteTurnBody = {
    content,
    ...(images.length > 0 && { images }),
    ...(thinking !== undefined && { thinking }),
  };
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
