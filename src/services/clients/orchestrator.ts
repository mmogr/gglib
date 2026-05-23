/**
 * Orchestrator service client.
 *
 * Provides:
 * - `planOrchestrator()` — SSE stream for `POST /api/orchestrator/plan`
 * - `runOrchestrator()`  — SSE stream for `POST /api/orchestrator/run`
 *
 * Uses `getAuthenticatedFetchConfig()` so it works in both Web and Tauri
 * modes transparently (HTTP transport — no tauri::command).
 *
 * @module services/clients/orchestrator
 */

import { getAuthenticatedFetchConfig } from '../transport/api/client';
import type {
  ApprovalDecisionPayload,
  OrchestratorEvent,
  OrchestratorRun,
  OrchestratorRunEvent,
  OrchestratorRunStatus,
} from '../../types/orchestrator';

// ─── Types ───────────────────────────────────────────────────────────────────

export interface PlanOrchestratorParams {
  goal: string;
  port: number;
  model?: string;
  max_replans?: number;
}

export interface RunOrchestratorParams {
  goal: string;
  port: number;
  model?: string;
  max_replans?: number;
  max_worker_concurrency?: number;
  hitl_mode?: string;
}

// ─── Client ──────────────────────────────────────────────────────────────────

/**
 * Call `POST /api/orchestrator/plan` and consume the SSE stream, calling
 * `onEvent` for each parsed `OrchestratorEvent`.
 *
 * Resolves when the stream ends; throws on HTTP errors.
 */
export async function planOrchestrator(
  params: PlanOrchestratorParams,
  onEvent: (event: OrchestratorEvent) => void,
  signal?: AbortSignal,
): Promise<void> {
  const { baseUrl, headers } = await getAuthenticatedFetchConfig();

  const response = await fetch(`${baseUrl}/api/orchestrator/plan`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      ...(headers as Record<string, string>),
    },
    body: JSON.stringify(params),
    signal,
  });

  if (!response.ok) {
    const body = await response.json().catch(() => ({}));
    throw new Error(
      (body as { error?: string }).error ?? `Orchestrator plan failed: ${response.status}`,
    );
  }

  const reader = response.body?.getReader();
  if (!reader) throw new Error('No response body for orchestrator SSE stream');

  const decoder = new TextDecoder();
  let buffer = '';

  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;

    buffer += decoder.decode(value, { stream: true });

    // SSE events are separated by double newlines.
    const parts = buffer.split('\n\n');
    buffer = parts.pop() ?? '';

    for (const part of parts) {
      for (const line of part.split('\n')) {
        if (line.startsWith('data:')) {
          const json = line.slice(5).trim();
          if (json) {
            try {
              onEvent(JSON.parse(json) as OrchestratorEvent);
            } catch {
              // skip malformed events
            }
          }
        }
      }
    }
  }
}

/**
 * Call `POST /api/orchestrator/run` and consume the SSE stream, calling
 * `onEvent` for each parsed `OrchestratorEvent`.
 *
 * This drives the full Director/Worker pipeline: planning, worker execution,
 * compaction, and synthesis.  Resolves when the stream ends (after
 * `orchestrator_complete` or `orchestrator_error`); throws on HTTP errors.
 */
export async function runOrchestrator(
  params: RunOrchestratorParams,
  onEvent: (event: OrchestratorEvent) => void,
  signal?: AbortSignal,
): Promise<void> {
  const { baseUrl, headers } = await getAuthenticatedFetchConfig();

  const response = await fetch(`${baseUrl}/api/orchestrator/run`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      ...(headers as Record<string, string>),
    },
    body: JSON.stringify(params),
    signal,
  });

  if (!response.ok) {
    const body = await response.json().catch(() => ({}));
    throw new Error(
      (body as { error?: string }).error ?? `Orchestrator run failed: ${response.status}`,
    );
  }

  const reader = response.body?.getReader();
  if (!reader) throw new Error('No response body for orchestrator SSE stream');

  const decoder = new TextDecoder();
  let buffer = '';

  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;

    buffer += decoder.decode(value, { stream: true });

    const parts = buffer.split('\n\n');
    buffer = parts.pop() ?? '';

    for (const part of parts) {
      for (const line of part.split('\n')) {
        if (line.startsWith('data:')) {
          const json = line.slice(5).trim();
          if (json) {
            try {
              onEvent(JSON.parse(json) as OrchestratorEvent);
            } catch {
              // skip malformed events
            }
          }
        }
      }
    }
  }
}

// ─── Phase D: HITL + Runs ────────────────────────────────────────────────────

/**
 * POST /api/orchestrator/approve/{approvalId}
 *
 * Resolve a pending HITL approval gate.
 */
export async function approveOrchestrator(
  approvalId: string,
  payload: ApprovalDecisionPayload,
): Promise<void> {
  const { baseUrl, headers } = await getAuthenticatedFetchConfig();
  const response = await fetch(
    `${baseUrl}/api/orchestrator/approve/${encodeURIComponent(approvalId)}`,
    {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(headers as Record<string, string>),
      },
      body: JSON.stringify(payload),
    },
  );
  if (!response.ok) {
    const body = await response.json().catch(() => ({}));
    throw new Error(
      (body as { error?: string }).error ?? `Approve request failed: ${response.status}`,
    );
  }
}

/**
 * GET /api/orchestrator/runs[?status=<status>]
 *
 * List orchestrator runs, optionally filtered by status.
 */
export async function listOrchestratorRuns(
  status?: OrchestratorRunStatus,
): Promise<OrchestratorRun[]> {
  const { baseUrl, headers } = await getAuthenticatedFetchConfig();
  const url = status
    ? `${baseUrl}/api/orchestrator/runs?status=${encodeURIComponent(status)}`
    : `${baseUrl}/api/orchestrator/runs`;
  const response = await fetch(url, { headers: headers as Record<string, string> });
  if (!response.ok) {
    throw new Error(`List runs failed: ${response.status}`);
  }
  const data = (await response.json()) as { runs: OrchestratorRun[] };
  return data.runs;
}

/**
 * GET /api/orchestrator/runs/{id}
 *
 * Get a single run with its events.
 */
export async function getOrchestratorRun(
  id: string,
): Promise<{ run: OrchestratorRun; events: OrchestratorRunEvent[] }> {
  const { baseUrl, headers } = await getAuthenticatedFetchConfig();
  const response = await fetch(
    `${baseUrl}/api/orchestrator/runs/${encodeURIComponent(id)}`,
    { headers: headers as Record<string, string> },
  );
  if (!response.ok) {
    throw new Error(`Get run failed: ${response.status}`);
  }
  return response.json() as Promise<{ run: OrchestratorRun; events: OrchestratorRunEvent[] }>;
}

/**
 * POST /api/orchestrator/runs/{id}/resume
 *
 * Resume a previously interrupted or awaiting-approval run as an SSE stream.
 */
export async function resumeOrchestratorRun(
  id: string,
  port: number,
  model?: string,
  onEvent?: (event: OrchestratorEvent) => void,
  signal?: AbortSignal,
): Promise<void> {
  const { baseUrl, headers } = await getAuthenticatedFetchConfig();
  const response = await fetch(
    `${baseUrl}/api/orchestrator/runs/${encodeURIComponent(id)}/resume`,
    {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(headers as Record<string, string>),
      },
      body: JSON.stringify({ port, model }),
      signal,
    },
  );
  if (!response.ok) {
    const body = await response.json().catch(() => ({}));
    throw new Error(
      (body as { error?: string }).error ?? `Resume failed: ${response.status}`,
    );
  }
  if (!onEvent) return;

  const reader = response.body?.getReader();
  if (!reader) throw new Error('No response body for resume SSE stream');

  const decoder = new TextDecoder();
  let buffer = '';

  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    const parts = buffer.split('\n\n');
    buffer = parts.pop() ?? '';
    for (const part of parts) {
      for (const line of part.split('\n')) {
        if (line.startsWith('data:')) {
          const json = line.slice(5).trim();
          if (json) {
            try {
              onEvent(JSON.parse(json) as OrchestratorEvent);
            } catch {
              // skip malformed events
            }
          }
        }
      }
    }
  }
}

// ─── Phase M: Rewind ─────────────────────────────────────────────────────────

export interface RewindOrchestratorParams {
  port: number;
  model?: string;
  wave_index: number;
  steering_note?: string;
}

/**
 * POST /api/orchestrator/runs/{id}/rewind
 *
 * Rewind a run to a previous wave and re-execute from there.
 * Streams new events via SSE.
 */
export async function rewindOrchestratorRun(
  id: string,
  params: RewindOrchestratorParams,
  onEvent?: (event: OrchestratorEvent) => void,
  signal?: AbortSignal,
): Promise<void> {
  const { baseUrl, headers } = await getAuthenticatedFetchConfig();
  const response = await fetch(
    `${baseUrl}/api/orchestrator/runs/${encodeURIComponent(id)}/rewind`,
    {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(headers as Record<string, string>),
      },
      body: JSON.stringify(params),
      signal,
    },
  );
  if (!response.ok) {
    const body = await response.json().catch(() => ({}));
    throw new Error(
      (body as { error?: string }).error ?? `Rewind failed: ${response.status}`,
    );
  }
  if (!onEvent) return;

  const reader = response.body?.getReader();
  if (!reader) throw new Error('No response body for rewind SSE stream');

  const decoder = new TextDecoder();
  let buffer = '';

  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    buffer += decoder.decode(value, { stream: true });
    const parts = buffer.split('\n\n');
    buffer = parts.pop() ?? '';
    for (const part of parts) {
      for (const line of part.split('\n')) {
        if (line.startsWith('data:')) {
          const json = line.slice(5).trim();
          if (json) {
            try {
              onEvent(JSON.parse(json) as OrchestratorEvent);
            } catch {
              // skip malformed events
            }
          }
        }
      }
    }
  }
}
