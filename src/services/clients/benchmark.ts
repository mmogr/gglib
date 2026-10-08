/**
 * Benchmark service client.
 *
 * Provides REST + SSE access to the benchmark API endpoints, through the
 * transport client: `get`/`post` for the plain calls, and `apiFetch` with the
 * shared SSE reader (`utils/sse`) for the four runs that stream.
 *
 * @module services/clients/benchmark
 */

import { apiFetch, get, post } from '../transport/api/client';
import { readSse } from '../../utils/sse';
import type {
  AgenticEvalConfig,
  AgenticEvalReport,
  ApplyOutcome,
  BenchmarkEvent,
  BenchmarkRun,
  CompareConfig,
  ListRunsResponse,
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
  const response = await get<ListRunsResponse>(
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

// ─── SSE endpoints ────────────────────────────────────────────────────────────

/**
 * POST `config` to `/api/benchmark/{run}` and hand each event of the reply to
 * `onEvent`, resolving when the stream ends.
 *
 * A request the daemon refuses rejects with the transport's `TransportError`.
 * A stream that breaks, or `signal` firing, rejects with what the read threw,
 * as a run's stream does. A payload that is not a JSON event is skipped.
 */
async function streamRun(
  run: 'compare' | 'perf' | 'tune' | 'agentic',
  config: unknown,
  onEvent: (event: BenchmarkEvent) => void,
  signal?: AbortSignal,
): Promise<void> {
  const response = await apiFetch(`/api/benchmark/${run}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(config),
    signal,
  });

  for await (const { data } of readSse(response)) {
    try {
      onEvent(JSON.parse(data) as BenchmarkEvent);
    } catch {
      // skip malformed events
    }
  }
}

/**
 * POST /api/benchmark/compare  (SSE)
 * Start a compare run for the given config and stream events via `onEvent`.
 * Resolves when the stream ends; throws on HTTP errors.
 */
export function startCompareRun(
  config: CompareConfig,
  onEvent: (event: BenchmarkEvent) => void,
  signal?: AbortSignal,
): Promise<void> {
  return streamRun('compare', config, onEvent, signal);
}

/**
 * POST /api/benchmark/perf  (SSE)
 * Start a perf run for the given config and stream events via `onEvent`.
 * Resolves when the stream ends; throws on HTTP errors.
 */
export function startPerfRun(
  config: PerfConfig,
  onEvent: (event: BenchmarkEvent) => void,
  signal?: AbortSignal,
): Promise<void> {
  return streamRun('perf', config, onEvent, signal);
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
export function startTuneRun(
  config: TuneConfig,
  onEvent: (event: BenchmarkEvent) => void,
  signal?: AbortSignal,
): Promise<void> {
  return streamRun('tune', config, onEvent, signal);
}

/**
 * POST /api/benchmark/agentic  (SSE)
 * Start a raw-vs-gglib agentic eval and stream events via `onEvent`.
 * Aborting the signal genuinely cancels the server-side run (the stream
 * guard drop-cancels the eval task). Resolves when the stream ends.
 */
export function startAgenticRun(
  config: AgenticEvalConfig,
  onEvent: (event: BenchmarkEvent) => void,
  signal?: AbortSignal,
): Promise<void> {
  return streamRun('agentic', config, onEvent, signal);
}

/**
 * POST /api/benchmark/tune/{runId}/apply — judge a completed tune run
 * against the apply gate; only an `apply` verdict has written the model.
 * Refusals come back as verdicts, never as HTTP errors.
 */
export async function applyTuneRun(runId: number): Promise<ApplyOutcome> {
  return post<ApplyOutcome>(`/api/benchmark/tune/${runId}/apply`);
}
