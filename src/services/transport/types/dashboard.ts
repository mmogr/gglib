/**
 * Proxy dashboard transport types.
 *
 * The frontend connects directly to an already-running proxy's own HTTP port
 * (`http://{host}:{port}/v1/proxy/status/stream`), the same way the CLI's
 * `gglib proxy dashboard` command does (see
 * `crates/gglib-cli/src/handlers/proxy_dashboard.rs`) — a real HTTP client of
 * the JSON contract. Unknown or extra fields are ignored by TypeScript's
 * structural typing, so a reader tolerates additive server-side changes the
 * same way the CLI's `serde(default)` does.
 *
 * These were hand-written declarations and are now aliases over
 * `src/types/generated/`. That tolerance is what let them drift: the mirror
 * was thirty-two fields out of step on optionality alone, and a client that
 * only ever reads the wire never notices being wrong about what is optional.
 *
 * The `Sampling*` names are the GUI's own. Rust calls several of these by
 * short names that are unambiguous inside `audit_records.rs` and are not out
 * here — `Divergence`, `BaselineReport`, `EffortRung` — so the aliases keep
 * the longer spellings the components use.
 *
 * `tokensInUse` used to live here. It is behaviour rather than a declaration
 * and now sits in `../slotTokens`, so that replacing these mirrors with
 * generated types did not have to step around live logic.
 */

// ============================================================================
// Connections and slots
// ============================================================================

export type { ConnectionPhase } from '../../../types/generated/ConnectionPhase';
export type { ActiveConnectionSnapshot } from '../../../types/generated/ActiveConnectionSnapshot';
export type { NextTokenInfo } from '../../../types/generated/NextTokenInfo';

/**
 * One llama-server slot.
 *
 * Gained `params` — llama-server's own parsed `temperature`, `top_p`, `top_k`,
 * `seed` and sampler chain, serialized on every slot and absent from the
 * mirror for its whole life. It is the only place the dashboard can show what
 * the *server* believes it was asked for, as against what gglib believes it
 * sent.
 */
export type { SlotSnapshot } from '../../../types/generated/SlotSnapshot';
export type { SlotParams } from '../../../types/generated/SlotParams';

// ============================================================================
// Context and cache
// ============================================================================

/**
 * One model's context accounting.
 *
 * Gained `tool_repaired`: how many tool calls this model needed re-issued
 * after failing schema validation. The proxy has always sent it.
 */
export type { ContextSnapshot } from '../../../types/generated/ContextSnapshot';

export type { CacheUsage } from '../../../types/generated/CacheUsage';

import type { CacheStatus } from '../../../types/generated/CacheStatus';

/**
 * KV cache health.
 *
 * `ram_state` is a closed union rather than a bare `string` because the Rust
 * field is a `&'static str` carrying a `#[ts(type = …)]` override. The
 * dashboard's switch is written against those five values and would lose
 * exhaustiveness without it.
 */
export type { CacheStatus };

export type { KvCacheType } from '../../../types/generated/KvCacheType';

/**
 * The five values `ram_state` can hold.
 *
 * Kept as a name because the generator inlines the union into the field and
 * there is no binding to import — the same reason `SecondarySlotState` exists
 * next door. A consumer writing a `Record` or a `switch` over these needs
 * something to name.
 */
export type CacheRamState = CacheStatus['ram_state'];

// ============================================================================
// Launch
// ============================================================================

export type { LaunchDecision } from '../../../types/generated/LaunchDecision';
export type { LaunchNarration } from '../../../types/generated/LaunchNarration';

// ============================================================================
// Sampling audit
// ============================================================================
//
// ADR 0007's record of what the proxy actually sent, as against what the
// resolution ladder decided it should.

export type { Divergence as SamplingDivergence } from '../../../types/generated/Divergence';
export type { BaselineVerdict as SamplingBaselineVerdict } from '../../../types/generated/BaselineVerdict';
export type { BaselineField as SamplingBaselineField } from '../../../types/generated/BaselineField';
export type { BaselineCoverage as SamplingBaselineCoverage } from '../../../types/generated/BaselineCoverage';
export type { BaselineReport as SamplingBaselineReport } from '../../../types/generated/BaselineReport';
export type { BaselineState as SamplingBaselineState } from '../../../types/generated/BaselineState';

/**
 * What one model's requests actually carried.
 *
 * Gained `effort_suppressed` — the record ADR 0007 says nothing else in the
 * system can reconstruct, because a `reasoning_effort` dropped by the template
 * gate leaves no other trace. The GUI had no way to render it.
 */
export type { SamplingAuditSnapshot } from '../../../types/generated/SamplingAuditSnapshot';
export type { EffortSuppressions } from '../../../types/generated/EffortSuppressions';
export type { SuppressedEffortRecord } from '../../../types/generated/SuppressedEffortRecord';

// ============================================================================
// Reasoning readback
// ============================================================================

export type { EffortSupportState as SamplingEffortSupport } from '../../../types/generated/EffortSupportState';
export type { EffortRung as SamplingEffortRung } from '../../../types/generated/EffortRung';
export type { BudgetRung as SamplingBudgetRung } from '../../../types/generated/BudgetRung';
export type { ResolvedReasoning as SamplingResolvedReasoning } from '../../../types/generated/ResolvedReasoning';
export type { ReasoningReadback as SamplingReasoningReadback } from '../../../types/generated/ReasoningReadback';
export type { ClientFieldTally as SamplingClientFieldTally } from '../../../types/generated/ClientFieldTally';
export type { ClientFieldNames as SamplingClientFieldNames } from '../../../types/generated/ClientFieldNames';

// ============================================================================
// Published overrides
// ============================================================================

export type { PublishedOverrides as SamplingPublishedOverrides } from '../../../types/generated/PublishedOverrides';
export type { PublishedOverrideField as SamplingPublishedField } from '../../../types/generated/PublishedOverrideField';
export type { PublishedOverrideState } from '../../../types/generated/PublishedOverrideState';

// ============================================================================
// Admission
// ============================================================================
//
// Re-exported rather than imported from `./admission` by each consumer: the
// snapshot embeds them, so a component reading one reaches for it here.

export type {
  AdmissionSnapshot,
  QueuedModelSnapshot,
  ResidentSlotSnapshot,
  SecondarySlotState,
  SecondarySlotStatus,
} from './admission';

// ============================================================================
// The snapshot itself
// ============================================================================

/**
 * Everything the proxy reports in one frame.
 *
 * Gained four fields the proxy has been sending all along and the mirror never
 * declared: `tool_repairs_attempted`, `tool_repairs_succeeded`,
 * `upstream_health` and `per_model_defects`.
 */
export type { DashboardSnapshot } from '../../../types/generated/DashboardSnapshot';
