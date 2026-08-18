/**
 * Benchmark service client.
 *
 * Provides REST + SSE access to the benchmark API endpoints.
 * Uses `getAuthenticatedFetchConfig()` for platform-agnostic auth headers
 * (works in both Tauri and web mode).
 *
 * Manual SSE parser over a chunked `TextDecoder` buffer.
 *
 * @module services/clients/benchmark
 */

import { get, getAuthenticatedFetchConfig } from '../transport/api/client';
import type {
  AgenticEvalConfig,
  AgenticEvalReport,
  ApplyOutcome,
  BenchmarkEvent,
  BenchmarkRun,
  CompareConfig,
  ListBenchmarkRunsResponse,
  ModelAgenticHistoryResponse,
  PerfConfig,
  TuneConfig,
} from '../../types/benchmark';

// ─── REST endpoints ───────────────────────────────────────────────────────────

/**
 * GET /api/benchmark/runs
 * List recent benchmark runs (paginated).
 */
export async function listBenchmarkRuns(
  limit = 20,
  offset = 0,
): Promise<BenchmarkRun[]> {
  const response = await get<ListBenchmarkRunsResponse>(
    `/api/benchmark/runs?limit=${limit}&offset=${offset}`,
  );
  return response.runs;
}

/**
 * GET /api/models/{id}/agentic-history — past raw-vs-gglib reports,
 * most recent first. `limit` is clamped to 1..=100 server-side.
 */
export async function getModelAgenticHistory(
  modelId: number,
  limit = 20,
): Promise<AgenticEvalReport[]> {
  const response = await get<ModelAgenticHistoryResponse>(
    `/api/models/${modelId}/agentic-history?limit=${limit}`,
  );
  return response.reports;
}

// ─── SSE streaming helpers ────────────────────────────────────────────────────

/**
 * Shared SSE reader — parses a `text/event-stream` response body and calls
 * `onEvent` for each complete `data:` line.  Resolves when the stream ends.
 */
async function consumeSseStream(
  response: Response,
  onEvent: (event: BenchmarkEvent) => void,
): Promise<void> {
  const reader = response.body?.getReader();
  if (!reader) throw new Error('No response body for benchmark SSE stream');

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
              onEvent(JSON.parse(json) as BenchmarkEvent);
            } catch {
              // skip malformed events
            }
          }
        }
      }
    }
  }
}

// ─── SSE endpoints ────────────────────────────────────────────────────────────

/**
 * POST /api/benchmark/compare  (SSE)
 * Start a compare run for the given config and stream events via `onEvent`.
 * Resolves when the stream ends; throws on HTTP errors.
 */
export async function startCompareRun(
  config: CompareConfig,
  onEvent: (event: BenchmarkEvent) => void,
  signal?: AbortSignal,
): Promise<void> {
  const { baseUrl, headers } = await getAuthenticatedFetchConfig();

  const response = await fetch(`${baseUrl}/api/benchmark/compare`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      ...(headers as Record<string, string>),
    },
    body: JSON.stringify(config),
    signal,
  });

  if (!response.ok) {
    const body = await response.json().catch(() => ({}));
    throw new Error(
      (body as { error?: string }).error ??
        `Compare run failed: ${response.status}`,
    );
  }

  await consumeSseStream(response, onEvent);
}

/**
 * POST /api/benchmark/perf  (SSE)
 * Start a perf run for the given config and stream events via `onEvent`.
 * Resolves when the stream ends; throws on HTTP errors.
 */
export async function startPerfRun(
  config: PerfConfig,
  onEvent: (event: BenchmarkEvent) => void,
  signal?: AbortSignal,
): Promise<void> {
  const { baseUrl, headers } = await getAuthenticatedFetchConfig();

  const response = await fetch(`${baseUrl}/api/benchmark/perf`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      ...(headers as Record<string, string>),
    },
    body: JSON.stringify(config),
    signal,
  });

  if (!response.ok) {
    const body = await response.json().catch(() => ({}));
    throw new Error(
      (body as { error?: string }).error ??
        `Perf run failed: ${response.status}`,
    );
  }

  await consumeSseStream(response, onEvent);
}

/**
 * POST /api/benchmark/tune  (SSE)
 * Start a tune run for the given config and stream events via `onEvent`.
 * Resolves when the stream ends; throws on HTTP errors.
 *
 * `config.task_suite` accepts either `{ source: 'default' }` or
 * `{ source: 'custom', tasks: TuneTask[] }` — for a custom suite, parse the
 * user's uploaded JSON file client-side into a plain `TuneTask[]` array
 * (the same shape `gglib benchmark tune --task-suite path.json` reads from
 * disk) and wrap it in the `custom` shape before calling this function.
 */
export async function startTuneRun(
  config: TuneConfig,
  onEvent: (event: BenchmarkEvent) => void,
  signal?: AbortSignal,
): Promise<void> {
  const { baseUrl, headers } = await getAuthenticatedFetchConfig();

  const response = await fetch(`${baseUrl}/api/benchmark/tune`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      ...(headers as Record<string, string>),
    },
    body: JSON.stringify(config),
    signal,
  });

  if (!response.ok) {
    const body = await response.json().catch(() => ({}));
    throw new Error(
      (body as { error?: string }).error ??
        `Tune run failed: ${response.status}`,
    );
  }

  await consumeSseStream(response, onEvent);
}

/**
 * POST /api/benchmark/agentic  (SSE)
 * Start a raw-vs-gglib agentic eval and stream events via `onEvent`.
 * Aborting the signal genuinely cancels the server-side run (the stream
 * guard drop-cancels the eval task). Resolves when the stream ends.
 */
export async function startAgenticRun(
  config: AgenticEvalConfig,
  onEvent: (event: BenchmarkEvent) => void,
  signal?: AbortSignal,
): Promise<void> {
  const { baseUrl, headers } = await getAuthenticatedFetchConfig();

  const response = await fetch(`${baseUrl}/api/benchmark/agentic`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      ...(headers as Record<string, string>),
    },
    body: JSON.stringify(config),
    signal,
  });

  if (!response.ok) {
    const body = await response.json().catch(() => ({}));
    throw new Error(
      (body as { error?: string }).error ??
        `Agentic eval failed: ${response.status}`,
    );
  }

  await consumeSseStream(response, onEvent);
}

/**
 * POST /api/benchmark/tune/{runId}/apply — judge a completed tune run
 * against the apply gate; only an `apply` verdict has written the model.
 * Refusals come back as verdicts, never as HTTP errors.
 */
export async function applyTuneRun(runId: number): Promise<ApplyOutcome> {
  const { baseUrl, headers } = await getAuthenticatedFetchConfig();
  const response = await fetch(`${baseUrl}/api/benchmark/tune/${runId}/apply`, {
    method: 'POST',
    headers: headers as Record<string, string>,
  });
  if (!response.ok) {
    const body = await response.json().catch(() => ({}));
    throw new Error(
      (body as { error?: string }).error ?? `apply failed: ${response.status}`,
    );
  }
  return (await response.json()) as ApplyOutcome;
}
