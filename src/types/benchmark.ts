// ============================================================================
// Benchmark Domain Types
// ============================================================================
//
// Mirrors the Rust `gglib-app-services::benchmark` domain types.
// Serde config on the Rust side:
//   - BenchmarkEvent: `#[serde(tag = "type", rename_all = "snake_case")]`
//   - BenchmarkModelResult: `#[serde(tag = "kind", rename_all = "snake_case")]`
//   - All structs: `#[serde(rename_all = "snake_case")]`
//
// @module types/benchmark

// ─── Enumerations ────────────────────────────────────────────────────────────

export type BenchmarkRunType = 'compare' | 'perf';
export type BenchmarkRunStatus = 'running' | 'complete' | 'failed';

// ─── Domain Entities ─────────────────────────────────────────────────────────

export interface BenchmarkRun {
  id: number;
  run_type: BenchmarkRunType;
  status: BenchmarkRunStatus;
  model_ids: number[];
  prompt_text?: string | null;
  system_prompt?: string | null;
  config_json?: string | null;
  error?: string | null;
  created_at: string;       // ISO 8601 UTC
  completed_at?: string | null;
}

export interface ModelCompareResult {
  id?: number | null;
  model_id: number;
  run_id?: number | null;
  prompt_text: string;
  system_prompt?: string | null;
  response_text: string;
  was_truncated: boolean;
  prompt_tokens?: number | null;
  completion_tokens?: number | null;
  prompt_ms?: number | null;
  generation_ms?: number | null;
  prompt_tps?: number | null;
  generation_tps?: number | null;
  created_at: string;
}

export interface ModelPerfResult {
  id?: number | null;
  model_id: number;
  run_id?: number | null;
  pp_tps: number;
  tg_tps: number;
  pp_tokens: number;
  tg_tokens: number;
  backend?: string | null;
  ngl?: number | null;
  context_size?: number | null;
  repetitions: number;
  created_at: string;
}

export interface ModelBenchmarkSummary {
  model_id: number;
  best_tg_tps?: number | null;
  best_pp_tps?: number | null;
  latest_tg_tps?: number | null;
  latest_pp_tps?: number | null;
  latest_backend?: string | null;
  perf_run_count: number;
  compare_run_count: number;
  last_benchmarked_at: string;
  updated_at: string;
}

// ─── Request Configs ─────────────────────────────────────────────────────────

export interface CompareConfig {
  model_ids: number[];
  prompt: string;
  system_prompt?: string | null;
  inference?: Record<string, unknown> | null;
  ctx_size?: number | null;
}

export interface PerfConfig {
  model_ids: number[];
  pp_tokens?: number | null;
  tg_tokens?: number | null;
  repetitions?: number | null;
}

// ─── SSE Event Discriminated Union ───────────────────────────────────────────

/** Payload of a `model_complete` event; tagged by `kind`. */
export type BenchmarkModelResult =
  | ({ kind: 'compare' } & ModelCompareResult)
  | ({ kind: 'perf' } & ModelPerfResult);

/**
 * Discriminated union of all SSE events emitted by the benchmark SSE stream.
 * Serde: `#[serde(tag = "type", rename_all = "snake_case")]`
 */
export type BenchmarkEvent =
  | { type: 'model_started'; model_id: number; model_name: string; position: number; total: number }
  | { type: 'model_text_delta'; model_id: number; text: string }
  | { type: 'model_complete'; model_id: number; result: BenchmarkModelResult }
  | { type: 'model_failed'; model_id: number; model_name: string; error: string }
  | { type: 'run_complete'; run_id: number }
  | { type: 'run_failed'; error: string };

// ─── API Response Shapes ─────────────────────────────────────────────────────

export interface ListBenchmarkRunsResponse {
  runs: BenchmarkRun[];
}

export interface GetBenchmarkRunResponse {
  run: BenchmarkRun;
}

export interface ModelBenchmarkHistoryResponse {
  summary?: ModelBenchmarkSummary | null;
  compare_history: ModelCompareResult[];
  perf_history: ModelPerfResult[];
}
