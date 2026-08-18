/**
 * Proxy dashboard transport types.
 *
 * These types mirror the JSON contract produced by
 * `gglib_proxy::dashboard::DashboardSnapshot` (Rust, `crates/gglib-proxy/src/dashboard.rs`)
 * bit-for-bit — field names and casing match the wire format exactly.
 *
 * The frontend connects directly to an already-running proxy's own HTTP port
 * (`http://{host}:{port}/v1/proxy/status/stream`), the same way the CLI's
 * `gglib proxy dashboard` command does (see
 * `crates/gglib-cli/src/handlers/proxy_dashboard.rs`) — a real HTTP client of
 * the JSON contract. Unknown or extra fields are ignored by TypeScript's
 * structural typing, so a reader tolerates additive server-side changes the
 * same way the CLI's `serde(default)` does.
 *
 * `tokensInUse` used to live here. It is behaviour rather than a declaration
 * and now sits in `../slotTokens`, so that replacing these mirrors with
 * generated types does not have to step around live logic.
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
 * `indeterminate` means the field could not be attributed — `/props` does not
 * report it, or something is masking the build's own value. It must never
 * render as agreement: that would report a tautology rather than an
 * observation. Before ADR 0003 deleted the sampler launch flags it was the
 * normal case for every field, because a flag overwrites the `/props` field it
 * names and the check was reading gglib's own value back.
 */
export type SamplingBaselineVerdict =
  | { verdict: 'matches' }
  | { verdict: 'differs'; expected: number; observed: number }
  /**
   * The effective default came from this model's own GGUF
   * (`general.sampling.*`), not the build. Neither agreement nor drift: the
   * model asked for it and llama.cpp applied it, which is gglib's deferral
   * working — it just means the build's own value is unobservable for that
   * field on this launch. Never render it as either of the other two.
   */
  | { verdict: 'model_supplied'; key: string; value: number }
  | { verdict: 'indeterminate'; reason: string };

/** Mirrors `gglib_proxy::props::BaselineField`. */
export interface SamplingBaselineField {
  field: string;
  verdict: SamplingBaselineVerdict;
}

/**
 * Mirrors `gglib_proxy::props::BaselineCoverage`
 * (`#[serde(tag = "coverage", rename_all = "snake_case")]`).
 *
 * Replaces a `conclusive: boolean` that was computed as "any field reached a
 * verdict", so a report covering two of seven fields claimed to be conclusive
 * and rendered as an all-clear over all seven. Only `complete` may render one.
 *
 * Orthogonal to drift: `complete` says every field was compared, not that
 * every field agreed. Check the field verdicts first.
 */
export type SamplingBaselineCoverage =
  | { coverage: 'complete' }
  | {
      coverage: 'partial';
      checked: number;
      /** Fields the model's own GGUF supplied, so the build's value is hidden. */
      model_supplied: number;
      indeterminate: number;
    }
  | { coverage: 'blind'; model_supplied: number; indeterminate: number };

/** Mirrors `gglib_proxy::props::BaselineReport`. */
export interface SamplingBaselineReport {
  fields: SamplingBaselineField[];
  coverage: SamplingBaselineCoverage;
}

/**
 * Mirrors `gglib_proxy::props::BaselineState`
 * (`#[serde(tag = "state", rename_all = "snake_case")]`).
 *
 * Three states rather than a nullable report, for `SamplingAuditState`'s
 * reason. `not_yet_read` and `unreadable` both mean "no table to show" and mean
 * opposite things: one is a poller that has not got there yet, the other is a
 * read that happened and failed. Rendering the second as the first is the same
 * blind-as-health collapse the slot half carries `blind { reason }` to avoid.
 */
export type SamplingBaselineState =
  | { state: 'not_yet_read' }
  | { state: 'unreadable'; reason: string }
  | { state: 'read'; report: SamplingBaselineReport };

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
  /**
   * `/props` baseline reading for the running model.
   *
   * Optional only so this mirror still parses a payload from a proxy that
   * predates the field; a current proxy always sends one of the three states.
   */
  baseline?: SamplingBaselineState | null;
  /**
   * What gglib's own requests do with the running model's published sampler
   * defaults.
   *
   * Optional only so this mirror still parses a payload from a proxy that
   * predates the field.
   */
  published?: SamplingPublishedOverrides | null;
  /**
   * The two reasoning controls: what the running template says about
   * `reasoning_effort`, what the last request resolved, and why none of it is
   * an observation.
   *
   * Optional only so this mirror still parses a payload from a proxy that
   * predates the field.
   */
  reasoning?: SamplingReasoningReadback | null;
  /**
   * Which of the client's own sampling fields were dropped, by name.
   *
   * `client_fields_discarded` above is the total; this is what it was made of.
   * Optional for a proxy that predates the field.
   */
  client_field_names?: SamplingClientFieldNames | null;
}

/**
 * Mirrors `gglib_proxy::audit_records::EffortSupportState`.
 *
 * The tri-state ADR 0007 decision 3 requires held all the way to the pixel:
 * `not_supported` is a positive observation that a resolved effort will be
 * suppressed, and `not_yet_observed` is nobody having managed to ask. A UI that
 * renders them the same way has re-introduced the collapse the type exists to
 * prevent — so neither may render as a green tick, and `not_yet_observed` may
 * never render as `not_supported`.
 *
 * These are the states *this build* knows; a newer proxy can send a fourth over
 * the HTTP contract above. So consumers must branch on `supported`
 * affirmatively and treat anything else as unknown — a catch-all arm here would
 * cost the narrowing that makes `reason` readable, so that floor lives in the
 * consumer's `default` branch (`ProxyReasoningRows.tsx`), as it does in the CLI
 * mirror's `#[serde(other)] Unrecognised`.
 */
export type SamplingEffortSupport =
  | { state: 'supported' }
  | { state: 'not_supported' }
  | { state: 'not_yet_observed'; reason: string };

/** Mirrors `gglib_proxy::audit_records::EffortRung`. */
export interface SamplingEffortRung {
  /** The level the ladder resolved, e.g. `high`. */
  level: string;
  /** The rung that supplied it — `profile`, `model`, `global`, `cli`. */
  source: string;
  /**
   * Whether the effort gate deleted it before sending. Rendering the level
   * without this marker reports a control that went nowhere as though it had
   * worked, and no readback exists that could contradict the claim.
   */
  suppressed: boolean;
}

/** Mirrors `gglib_proxy::audit_records::BudgetRung`. */
export interface SamplingBudgetRung {
  /** The cap, in tokens. `0` is the documented "stop thinking". */
  tokens: number;
  /** The rung that supplied it. */
  source: string;
}

/**
 * Mirrors `gglib_proxy::audit_records::ResolvedReasoning`.
 *
 * A record with both halves null is **not** the same as no record: it says a
 * request was resolved and named neither control, where an absent record says
 * no request has been resolved at all.
 */
export interface SamplingResolvedReasoning {
  effort?: SamplingEffortRung | null;
  budget?: SamplingBudgetRung | null;
}

/**
 * Mirrors `gglib_proxy::audit_records::ReasoningReadback`.
 *
 * Structurally unlike every other field on the audit snapshot: those report a
 * comparison between what gglib sent and what llama-server echoed, and there is
 * no echo for these two. `reasoning_effort` becomes a chat-template kwarg
 * consumed at render time, and `task_params::to_json` serialises no
 * `reasoning_budget_*` field (ADR 0007 finding 7a). So this is gglib's own
 * record, and `wire_blind_reason` is the server-supplied sentence that says so.
 */
export interface SamplingReasoningReadback {
  effort_support: SamplingEffortSupport;
  /** What the most recent resolved request named. `null` until one has been. */
  latest?: SamplingResolvedReasoning | null;
  /** Why nothing above is corroborated. Sent by the server, never paraphrased. */
  wire_blind_reason: string;
}

/** Mirrors `gglib_proxy::audit_records::ClientFieldTally`. */
export interface SamplingClientFieldTally {
  /** The wire key, as gglib names it — never a client-supplied string. */
  field: string;
  /** Times the trust gate binned it. Large by default and not a fault. */
  discarded: number;
  /** Times it could not be read as sent. */
  rejected: number;
}

/** Mirrors `gglib_proxy::audit_records::ClientFieldNames`. */
export interface SamplingClientFieldNames {
  /** Tracked names, most-dropped first. */
  fields: SamplingClientFieldTally[];
  /**
   * Drops whose name was not tracked because the bounded table was full. Zero
   * on every configuration gglib can currently produce, and reported anyway: a
   * silent bound and a bound nobody hit look identical.
   */
  untracked: number;
}

/**
 * Published-vs-sent for the running model.
 *
 * Mirrors `gglib_proxy::sampling_audit::PublishedOverrides`. A *different*
 * question from the baseline check above, and the two report the same field
 * differently without either being wrong: `/props` says `model_supplied`
 * because the *build's* value is unobservable there, while this says
 * `overridden` because gglib's request body wins over the table `/props`
 * renders. A reader seeing only the first would reasonably conclude the model's
 * value is what the sampler uses.
 */
export interface SamplingPublishedOverrides {
  /**
   * Resolved intents folded in since this model launched.
   *
   * **Zero means nothing has been compared, never "nothing is overridden".**
   * `fields` is empty both when a model publishes nothing and when no request
   * has been resolved yet, and those license opposite conclusions.
   */
  intents: number;
  /** One entry per field this model publishes. Empty on almost every model. */
  fields: SamplingPublishedField[];
}

/** One published field and what gglib's most recent intent did with it. */
export type SamplingPublishedField = {
  /** gglib's wire name for the parameter. */
  field: string;
  /** The GGUF key carrying it, e.g. `general.sampling.penalty_repeat`. */
  key: string;
} & (
  | { state: 'deferred'; published: number }
  | { state: 'restated'; published: number }
  | { state: 'overridden'; published: number; sending: number }
  | { state: 'unreadable' }
);

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
