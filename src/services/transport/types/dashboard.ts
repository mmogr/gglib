/**
 * Proxy dashboard transport types.
 *
 * These types mirror the JSON contract produced by
 * `gglib_proxy::dashboard::DashboardSnapshot` (Rust, `crates/gglib-proxy/src/dashboard.rs`)
 * bit-for-bit — field names and casing match the wire format exactly.
 *
 * This is a **local mirror, not a shared type** (there is of course no shared
 * Rust/TS type system across the wasm boundary here): the frontend connects
 * directly to an already-running proxy's own HTTP port
 * (`http://{host}:{port}/v1/proxy/status/stream`), the same way the CLI's
 * `gglib proxy dashboard` command does (see
 * `crates/gglib-cli/src/handlers/proxy_dashboard.rs`) — a real HTTP client of
 * the JSON contract, not a shared in-process type. Unknown/extra fields are
 * simply ignored by TypeScript's structural typing, so this mirror tolerates
 * additive server-side changes the same way the CLI's `serde(default)` does.
 */

/** Mirrors `gglib_proxy::connections::ConnectionPhase` (`#[serde(rename_all = "snake_case")]`). */
export type ConnectionPhase = 'queued' | 'processing_prompt' | 'generating';

/** Mirrors `gglib_proxy::connections::ActiveConnectionSnapshot`. */
export interface ActiveConnectionSnapshot {
  id: string;
  model_name: string;
  started_at_secs: number;
  is_streaming: boolean;
  num_ctx?: number | null;
  phase: ConnectionPhase;
  prompt_processed?: number | null;
  prompt_total?: number | null;
  prompt_cached?: number | null;
  prompt_time_ms?: number | null;
}

/** Mirrors `gglib_proxy::slots::NextTokenInfo` (a private-but-serialized field). */
export interface NextTokenInfo {
  n_decoded?: number | null;
}

/**
 * Mirrors `gglib_proxy::slots::SlotSnapshot`, including its private-but-serialized
 * legacy fields — serde ignores Rust visibility, so `n_past`/`cache_tokens`/
 * `next_token` all appear on the wire despite being private in Rust.
 */
export interface SlotSnapshot {
  id: number;
  id_task?: number | null;
  n_ctx?: number | null;
  is_processing: boolean;
  n_past?: number | null;
  cache_tokens?: number | null;
  next_token?: NextTokenInfo | null;
}

/**
 * Same priority-fallback chain as `SlotSnapshot::tokens_in_use()` (Rust) and
 * `proxy_dashboard.rs`'s local reimplementation (CLI) — kept in sync by hand,
 * since it's a tiny amount of logic mirrored across three consumers.
 */
export function tokensInUse(slot: SlotSnapshot): number | null {
  return slot.n_past ?? slot.cache_tokens ?? slot.next_token?.n_decoded ?? null;
}

/** Mirrors `gglib_proxy::metrics::ContextSnapshot`. */
export interface ContextSnapshot {
  model_name: string;
  payload_chars_before: number;
  payload_chars_after: number;
  messages_truncated: number;
  was_clamped: boolean;
  recorded_at_secs: number;
}

/** Mirrors `gglib_proxy::dashboard::DashboardSnapshot` — the full hydration/tick payload. */
export interface DashboardSnapshot {
  active_connections: ActiveConnectionSnapshot[];
  slots_available: boolean;
  slots: SlotSnapshot[];
  slots_status?: string | null;
  recent_requests: ContextSnapshot[];
  total_requests: number;
}
