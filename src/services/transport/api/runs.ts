/**
 * Runs: replies the daemon owns, at `/api/runs`.
 *
 * A run is started under an id the page mints, carries on whether or not
 * anything reads it, and can be read from its first event at any time while
 * the daemon keeps it. Reading stops when the caller aborts; that does not
 * stop the run. Only `cancelRun` does.
 */

import type { LocalTurn } from '../../../types/generated/LocalTurn';
import type { RunInfo } from '../../../types/generated/RunInfo';
import type { RunList } from '../../../types/generated/RunList';
import { readSse } from '../../../utils/sse';
import { apiFetch, get, post, put } from './client';

/**
 * One item of a run's stream: a logged frame, the run's latest preview frame
 * (sent beside the log with no seq: show it, never store it, keep only the
 * newest, drop it on its call's `tool_call_complete`), or the run's final
 * state.
 */
export type RunStreamItem =
  | { type: 'frame'; seq: number; data: string }
  | { type: 'preview'; toolCallId: string; data: string }
  | { type: 'end'; info: RunInfo };

function runPath(id: string): string {
  return `/api/runs/${encodeURIComponent(id)}`;
}

/**
 * Start this machine's turn under `id`, or get the run that already has it.
 * Refused before the run is made with the daemon's coded error:
 * `agent_busy` (429), `conversation_not_found` (404), `no_model` (422),
 * `folder_unusable`, `attachment_not_found`, `drawing_unavailable` and the
 * like; what is refused once the run exists ends it `failed`, with the code.
 */
export async function startAgentRun(id: string, turn: LocalTurn): Promise<RunInfo> {
  return put<RunInfo>(`${runPath(id)}?kind=agent`, turn);
}

/** Every run this machine has, newest first. */
export async function listRuns(): Promise<RunInfo[]> {
  return (await get<RunList>('/api/runs')).runs;
}

/** Cancel a run. Idempotent. */
export async function cancelRun(id: string): Promise<RunInfo> {
  return post<RunInfo>(`${runPath(id)}/cancel`);
}

/**
 * A run's events after `after` (0 for all of them), then its final state,
 * read until the daemon closes the stream or `signal` fires.
 */
export function readRunEvents(
  id: string,
  after: number,
  signal: AbortSignal,
): AsyncGenerator<RunStreamItem> {
  return readRunStream(`${runPath(id)}/events?after=${after}`, signal);
}

/** A run's stream at `path` on the daemon, as `readRunEvents` reads it. */
export async function* readRunStream(
  path: string,
  signal: AbortSignal,
): AsyncGenerator<RunStreamItem> {
  const response = await apiFetch(path, { headers: { Accept: 'text/event-stream' }, signal });

  for await (const event of readSse(response, signal)) {
    if (event.data === 'ping') continue; // a keepalive sent as data, not a frame
    if (event.event === 'run') {
      yield { type: 'end', info: JSON.parse(event.data) as RunInfo };
      return;
    }
    if (event.event === 'preview') {
      const preview = previewOf(event.data);
      if (preview) yield preview;
      continue;
    }
    yield { type: 'frame', seq: Number(event.id ?? 0), data: event.data };
  }
}

/** A preview event's item, or `null` when its data is not one. */
function previewOf(data: string): RunStreamItem | null {
  try {
    const parsed = JSON.parse(data) as { tool_call_id?: unknown };
    if (typeof parsed.tool_call_id !== 'string') return null;
    return { type: 'preview', toolCallId: parsed.tool_call_id, data };
  } catch {
    return null;
  }
}
