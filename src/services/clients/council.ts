/**
 * Council service client.
 *
 * Provides `suggestCouncil()` (JSON request/response) and `runCouncil()`
 * (SSE stream consumer) against the `/api/council/*` Axum endpoints.
 *
 * Uses `getAuthenticatedFetchConfig()` so it works in both Web and Tauri
 * modes transparently.
 *
 * @module services/clients/council
 */

import { getAuthenticatedFetchConfig } from '../transport/api/client';

// ─── Types (mirrors Rust council::config + council::events) ─────────────────

export interface CouncilAgent {
  id: string;
  name: string;
  color: string;
  persona: string;
  perspective: string;
  contentiousness: number;
  tool_filter?: string[];
}

export interface CouncilConfig {
  agents: CouncilAgent[];
  topic: string;
  rounds: number;
  synthesis_guidance?: string;
}

export interface SuggestedCouncil {
  agents: CouncilAgent[];
  rounds: number;
  synthesis_guidance?: string;
}

// ─── Suggest ────────────────────────────────────────────────────────────────

export interface SuggestCouncilParams {
  port: number;
  topic: string;
  agent_count?: number;
  model?: string;
  /** Previous suggestion to refine (multi-turn suggest). */
  previous_suggestion?: SuggestedCouncil;
  /** User's follow-up requesting changes to the prior suggestion. */
  refinement?: string;
}

/**
 * Ask the LLM to design a council for the given topic.
 *
 * Returns a `SuggestedCouncil` with agents, rounds, and optional
 * synthesis guidance.
 */
export async function suggestCouncil(
  params: SuggestCouncilParams,
  signal?: AbortSignal,
): Promise<SuggestedCouncil> {
  const { baseUrl, headers } = await getAuthenticatedFetchConfig();

  const response = await fetch(`${baseUrl}/api/council/suggest`, {
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
    throw new Error(body.error ?? `Council suggest failed: ${response.status}`);
  }

  return response.json();
}

// ─── Run (SSE stream) ───────────────────────────────────────────────────────

export interface RunCouncilParams {
  port: number;
  council: CouncilConfig;
  model?: string;
  config?: {
    max_iterations?: number;
    max_parallel_tools?: number;
    tool_timeout_ms?: number;
  };
}

/**
 * Start a council deliberation and stream `CouncilEvent`s via SSE.
 *
 * @param params  - Council run configuration.
 * @param onEvent - Called for each parsed `CouncilEvent`.
 * @param signal  - Optional abort signal to cancel the stream.
 * @returns A promise that resolves when the stream ends.
 */
export async function runCouncil(
  params: RunCouncilParams,
  onEvent: (event: Record<string, unknown>) => void,
  signal?: AbortSignal,
): Promise<void> {
  const { baseUrl, headers } = await getAuthenticatedFetchConfig();

  const response = await fetch(`${baseUrl}/api/council/run`, {
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
    throw new Error(body.error ?? `Council run failed: ${response.status}`);
  }

  const reader = response.body?.getReader();
  if (!reader) throw new Error('No response body for council SSE stream');

  const decoder = new TextDecoder();
  let buffer = '';

  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;

    buffer += decoder.decode(value, { stream: true });

    // SSE events are separated by double newlines
    const parts = buffer.split('\n\n');
    buffer = parts.pop() ?? '';

    for (const part of parts) {
      for (const line of part.split('\n')) {
        if (line.startsWith('data:')) {
          const json = line.slice(5).trim();
          if (json) {
            try {
              onEvent(JSON.parse(json));
            } catch {
              // skip malformed events
            }
          }
        }
      }
    }
  }
}
