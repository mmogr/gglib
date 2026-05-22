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
import type { OrchestratorEvent } from '../../types/orchestrator';

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
