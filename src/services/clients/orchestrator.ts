/**
 * Orchestrator service client.
 *
 * Provides `planOrchestrator()` (SSE stream consumer) against the
 * `POST /api/orchestrator/plan` Axum endpoint.
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
