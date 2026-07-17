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
 * `n_prompt_tokens(_processed)`/`next_token` all appear on the wire despite
 * being private in Rust.
 *
 * `next_token` is a single object on regular llama-server builds, but an
 * array of objects on builds with Multi-Token Prediction ("draft-mtp")
 * enabled — mirrors `gglib_proxy::slots::NextTokenField`.
 */
export interface SlotSnapshot {
  id: number;
  id_task?: number | null;
  n_ctx?: number | null;
  is_processing: boolean;
  n_past?: number | null;
  cache_tokens?: number | null;
  n_prompt_tokens?: number | null;
  n_prompt_tokens_processed?: number | null;
  n_prompt_tokens_cache?: number | null;
  next_token?: NextTokenInfo | NextTokenInfo[] | null;
}

/**
 * Same additive logic as `SlotSnapshot::tokens_in_use()` (Rust) and
 * `proxy_dashboard.rs`'s local reimplementation (CLI) — kept in sync by hand,
 * since it's a tiny amount of logic mirrored across three consumers.
 *
 * Current-schema builds report prompt usage and generation progress as two
 * separate counters — `n_prompt_tokens(_processed)` and `next_token.n_decoded`
 * — which must be added together to get the true total (a 20k-token prompt
 * with 89 tokens generated so far is ~20k tokens in use, not 89).
 * `n_prompt_tokens_processed` is preferred over `n_prompt_tokens` when both
 * are present (it tracks real progress mid-prefill) and, when present, is
 * combined with `n_prompt_tokens_cache` (tokens reused from KV cache this
 * round, not re-processed) — otherwise a cache-hit follow-up prompt would
 * falsely collapse context usage down to just the tiny newly-processed
 * delta. The grand-total `n_prompt_tokens` fallback (used only when
 * `_processed` is absent) already includes any cached prefix, so cache is
 * NOT added on top of it. Only when neither prompt-side field is present
 * does this fall back to the legacy, non-additive chain: `n_past`, then
 * `cache_tokens`, then `n_decoded` alone.
 *
 * `next_token` may be a single object or an array (MTP builds); element 0 is
 * the accepted/main decode stream when it's an array.
 */
export function tokensInUse(slot: SlotSnapshot): number | null {
  const nextToken = Array.isArray(slot.next_token) ? slot.next_token[0] : slot.next_token;
  const nDecoded = nextToken?.n_decoded ?? undefined;

  const promptComponent =
    slot.n_prompt_tokens_processed != null
      ? slot.n_prompt_tokens_processed + (slot.n_prompt_tokens_cache ?? 0)
      : slot.n_prompt_tokens;

  if (promptComponent != null) {
    return promptComponent + (nDecoded ?? 0);
  }

  return slot.n_past ?? slot.cache_tokens ?? nDecoded ?? null;
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
  /** Whether KV cache persistence is enabled on this proxy instance. */
  cache_enabled?: boolean;
}
