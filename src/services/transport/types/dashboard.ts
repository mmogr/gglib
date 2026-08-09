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

import type { AdmissionSnapshot } from './admission';

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

export type {
  AdmissionSnapshot,
  QueuedModelSnapshot,
  ResidentSlotSnapshot,
  SecondarySlotState,
  SecondarySlotStatus,
} from './admission';

/** Mirrors `gglib_proxy::metrics::ContextSnapshot`. */
export interface ContextSnapshot {
  model_name: string;
  payload_chars_before: number;
  payload_chars_after: number;
  messages_truncated: number;
  was_clamped: boolean;
  /** True when the pre-dispatch loop guard rejected this request with a 400. */
  loop_guard_tripped: boolean;
  /** True when the proxy originated a decode-time tool-call grammar. */
  grammar_enforced: boolean;
  /** True when dialect markup survived normalization into this request's client-visible output. */
  dialect_residue: boolean;
  recorded_at_secs: number;
}

/** Health of the resolved `--cache-ram` budget. Mirrors `CacheStatus.ram_state`. */
export type CacheRamState =
  | 'healthy'
  | 'low'
  | 'disabled_insufficient_ram'
  | 'disabled_by_user'
  | 'llama_default';

/**
 * Mirrors `gglib_core::cache_metrics::CacheUsage` — measured prompt-cache
 * reuse since the proxy started.
 *
 * Raw counts only. Nothing here is derived or estimated: reuse is exact, but
 * what it saved depends on a prefill that never ran, so no "time saved" figure
 * exists to display.
 */
export interface CacheUsage {
  /** Completed requests whose upstream reported a cached-token count. */
  reporting_requests: number;
  /**
   * Completed requests whose upstream omitted the field, excluded from every
   * other figure here. Lets a consumer tell "no reuse" from "no data".
   */
  unreported_requests: number;
  /** Total prompt tokens across `reporting_requests`. */
  prompt_tokens: number;
  /** Total prompt tokens served from cache. Always `<= prompt_tokens`. */
  cached_tokens: number;
  /** Prompt tokens in the most recent reporting request. */
  last_prompt_tokens?: number | null;
  /** Tokens reused from cache in the most recent reporting request. */
  last_cached_tokens?: number | null;
}

/**
 * Mirrors `gglib_proxy::dashboard::CacheStatus` — how prompt caching is
 * configured for the running model.
 *
 * The fields directly on this interface are configuration: resolved when a
 * model launches and changing only on a model swap. Per-request measurements
 * live under `usage`.
 */
export interface CacheStatus {
  /** Whether disk KV slot persistence is enabled on this proxy instance. */
  disk_enabled: boolean;
  /**
   * Disk layer enabled proxy-wide but suppressed for this model, whose
   * attention keeps only part of the token history. Always `false` when
   * `disk_enabled` is `false`.
   */
  disk_suppressed_for_model: boolean;
  /** Resolved `--cache-ram` budget in MiB; `null` when llama-server's own default applies. */
  ram_budget_mb?: number | null;
  /** Machine-readable budget health, for styling. */
  ram_state: CacheRamState;
  /** Whether anything here warrants surfacing to the user. */
  needs_attention: boolean;
  /** Ready-to-render warning lines; empty when nothing is wrong. */
  warnings: string[];
  /** Measured reuse since proxy start. Unlike the fields above, moves per request. */
  usage: CacheUsage;
}

/**
 * Mirrors `gglib_core::domain::LaunchDecision` — one resolved launch decision
 * and the reason it was chosen.
 *
 * `source` is the point of the type: a value alone says what happened, never
 * why. `null` only for decisions whose value states its own origin.
 */
export interface LaunchDecision {
  /** Stable key: `ctx`, `backend`, `kv`, `cache`, `mtp`, `flags`, `dialect`. */
  label: string;
  /** Display-ready value. */
  value: string;
  /** Provenance, rendered in parentheses. */
  source?: string | null;
}

/**
 * Mirrors `gglib_core::domain::LaunchNarration` — what the runtime decided
 * when it launched the running model, and why.
 *
 * The same record the CLI banner prints at startup. `decisions` arrives in
 * display order; consumers render it as given rather than re-sorting, so the
 * GUI and the banner cannot disagree about what a launch decided.
 */
export interface LaunchNarration {
  model_name: string;
  /** Quantization label (`Q4_K_M`); `null` when the catalog recorded none. */
  quantization?: string | null;
  /** On-disk weight size in bytes; `0` when unknown. */
  weights_bytes: number;
  decisions: LaunchDecision[];
}

/**
 * Mirrors `gglib_proxy::sampling_audit::AuditState`
 * (`#[serde(tag = "state", rename_all = "snake_case")]`).
 *
 * A tagged union rather than a count, deliberately. `blind` and
 * `{comparing, divergences: 0}` both mean "no divergences reported" and mean
 * opposite things: one is an instrument that cannot see, the other is one that
 * sees and finds nothing wrong. Consumers must render them differently — see
 * the Rust module docs on ADR 0002 finding 2's inert-module trap.
 */
export type SamplingAuditState =
  | { state: 'not_yet_observed' }
  | { state: 'blind'; reason: string }
  | { state: 'comparing'; comparisons: number; divergences: number };

/** Mirrors `gglib_proxy::sampling_audit::Divergence`. */
export interface SamplingDivergence {
  /** Wire name of the parameter. */
  field: string;
  /** What gglib resolved and wrote into the request body. */
  sent: number;
  /** What llama-server reported for the request in flight. */
  observed: number;
  /** Ladder rung the sent value came from, or `floor`/`unset`. */
  provenance: string;
}

/**
 * Mirrors `gglib_proxy::props::BaselineVerdict`
 * (`#[serde(tag = "verdict", rename_all = "snake_case")]`).
 *
 * `indeterminate` is the normal case today: gglib passes sampler values as
 * llama-server launch flags, and those overwrite the very `/props` table this
 * check reads. Rendering it as agreement would be reporting a tautology.
 */
export type SamplingBaselineVerdict =
  | { verdict: 'matches' }
  | { verdict: 'differs'; expected: number; observed: number }
  | { verdict: 'indeterminate'; reason: string };

/** Mirrors `gglib_proxy::props::BaselineField`. */
export interface SamplingBaselineField {
  field: string;
  verdict: SamplingBaselineVerdict;
}

/** Mirrors `gglib_proxy::props::BaselineReport`. */
export interface SamplingBaselineReport {
  fields: SamplingBaselineField[];
  /** `false` when nothing could be concluded about any field. */
  conclusive: boolean;
}

/**
 * Mirrors `gglib_proxy::sampling_audit::SamplingAuditSnapshot` — whether the
 * sampling gglib resolved is the sampling llama-server applied.
 */
export interface SamplingAuditSnapshot {
  state: SamplingAuditState;
  /**
   * Polls that saw busy slots but could not attribute them to one intent,
   * because the requests in flight had resolved differently and llama-server
   * gives no way to join a slot to the request that filled it.
   *
   * Beside the state, not inside it: abstaining is something a *sighted* organ
   * does, and a large count here alongside zero comparisons is a different
   * problem from blindness.
   */
  skipped_ambiguous: number;
  /** Client sampling fields that could not be read as sent. */
  client_fields_rejected: number;
  /**
   * Client sampling fields dropped by the trust gate. Expected to be large by
   * default — `trust_client_sampling` is off, so every client-supplied value
   * is discarded by design.
   */
  client_fields_discarded: number;
  /** Most recent field-level disagreements, oldest first. */
  recent_divergences: SamplingDivergence[];
  /** `/props` baseline reading for the running model; `null` before one is taken. */
  baseline?: SamplingBaselineReport | null;
}

/** Mirrors `gglib_proxy::dashboard::DashboardSnapshot` — the full hydration/tick payload. */
export interface DashboardSnapshot {
  active_connections: ActiveConnectionSnapshot[];
  slots_available: boolean;
  slots: SlotSnapshot[];
  slots_status?: string | null;
  recent_requests: ContextSnapshot[];
  total_requests: number;
  /** Requests whose client-visible output carried dialect residue (drift alarm), eviction-safe. */
  dialect_residue_total: number;
  /**
   * Prompt-cache configuration for the running model. `null` until the first
   * request resolves one, since the RAM budget isn't known until launch.
   *
   * Replaces the former `cache_enabled` boolean, which was declared here but
   * never read; `cache.disk_enabled` carries the same information.
   */
  cache?: CacheStatus | null;
  /**
   * Prompt-cache reuse for the in-process agent path (GUI chat),
   * reported alongside `cache.usage` and never merged into it. Top-level and
   * always present, since it does not depend on a resolved model; may be absent
   * on a proxy older than this field.
   */
  agent_usage?: CacheUsage | null;
  /**
   * What the running model's launch decided, and why. `null` until a request
   * resolves a model, and absent on a proxy older than this field.
   */
  launch?: LaunchNarration | null;
  /**
   * VRAM residency and the admission queue. Always present on a proxy that has
   * it; optional here so this mirror still parses a payload from one that
   * predates M9.
   */
  admission?: AdmissionSnapshot | null;
  /**
   * Whether the sampling gglib resolved reached llama-server intact, and
   * whether anyone is in a position to know. Optional here so this mirror
   * still parses a payload from a proxy that predates it.
   */
  sampling_audit?: SamplingAuditSnapshot | null;
}
