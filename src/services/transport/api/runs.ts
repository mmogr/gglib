/**
 * Runs: replies the daemon owns, at `/api/runs`.
 *
 * A run is started under an id the page mints, carries on whether or not
 * anything reads it, and can be read from its first event at any time while
 * the daemon keeps it. Reading stops when the caller aborts; that does not
 * stop the run. Only `cancelRun` does.
 */

import type { AgentRunRequest } from '../../../types/generated/AgentRunRequest';
import type { RunInfo } from '../../../types/generated/RunInfo';
import type { RunList } from '../../../types/generated/RunList';
import { readData } from '../errors';
import { get, post, put, getAuthenticatedFetchConfig } from './client';
import { readSseEvents } from './sseEvents';

/** One item of a run's stream: a logged frame, or the run's final state. */
export type RunStreamItem =
  | { type: 'frame'; seq: number; data: string }
  | { type: 'end'; info: RunInfo };

function runPath(id: string): string {
  return `/api/runs/${encodeURIComponent(id)}`;
}

/**
 * Start an agent run under `id`, or get the run that already has it. Refused
 * with the daemon's coded error: `agent_busy` (429), `conversation_not_found`
 * (404), `unavailable` (503) and the chat route's own.
 */
export async function startAgentRun(id: string, request: AgentRunRequest): Promise<RunInfo> {
  return put<RunInfo>(`${runPath(id)}?kind=agent`, request);
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
export async function* readRunEvents(
  id: string,
  after: number,
  signal: AbortSignal,
): AsyncGenerator<RunStreamItem> {
  const { baseUrl, headers } = await getAuthenticatedFetchConfig();
  const response = await fetch(`${baseUrl}${runPath(id)}/events?after=${after}`, {
    headers: { ...(headers as Record<string, string>), Accept: 'text/event-stream' },
    signal,
  });
  if (!response.ok) await readData(response);

  for await (const event of readSseEvents(response, signal)) {
    if (event.event === 'run') {
      yield { type: 'end', info: JSON.parse(event.data) as RunInfo };
      return;
    }
    yield { type: 'frame', seq: Number(event.id ?? 0), data: event.data };
  }
}
