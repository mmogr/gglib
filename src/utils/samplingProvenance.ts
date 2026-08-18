/**
 * Display helpers for the resolved-sampling section.
 *
 * The wording mirrors `gglib model explain`
 * (`crates/gglib-cli/src/presentation/explain_display.rs`) so the two surfaces
 * describe the same fact the same way. Kept apart from the component so each
 * mapping is directly testable.
 */

import type {
  DefaultsOriginName,
  InferenceConfig,
  ParamProvenance,
  ProvenanceParamKey,
  PublishedDefault,
  SamplingParamKey,
} from '../types';
import { UNKNOWN } from './format';

/**
 * Human-readable label per parameter, in the order the server sends them.
 *
 * Keyed by {@link ProvenanceParamKey} rather than `SamplingParamKey`: the
 * explain table is driven by what the server attributes a source to, and that
 * now includes both reasoning controls. Before they were listed here the table
 * rendered `reasoningEffort` as its own wire key beside every other row's
 * prose label.
 */
export const PARAM_LABELS: Record<ProvenanceParamKey, string> = {
  temperature: 'Temperature',
  topP: 'Top P',
  topK: 'Top K',
  presencePenalty: 'Presence Penalty',
  repeatPenalty: 'Repeat Penalty',
  minP: 'Min P',
  frequencyPenalty: 'Frequency Penalty',
  dynatempRange: 'Dynatemp Range',
  dynatempExponent: 'Dynatemp Exponent',
  topNSigma: 'Top-N-Sigma',
  dryMultiplier: 'DRY Multiplier',
  dryBase: 'DRY Base',
  dryAllowedLength: 'DRY Allowed Length',
  dryPenaltyLastN: 'DRY Penalty Last N',
  maxTokens: 'Max Tokens',
  reasoningEffort: 'Reasoning Effort',
  reasoningBudgetTokens: 'Reasoning Budget',
};

/** The facts about the model that change how a source reads. */
export interface SourceContext {
  /** The profile applied, if one was selected. */
  profile?: string | null;
  /** Selects which floor the coupled set falls back to. */
  isReasoning: boolean;
  /**
   * Where the model's stored defaults came from.
   *
   * The auto-detected and published rungs are the same rung, so naming it
   * needs this. Absent on a backend that predates the field, which reads as
   * the previous behaviour.
   */
  defaultsOrigin?: DefaultsOriginName | null;
}

/**
 * Describe where one resolved value came from.
 *
 * A layer index the server could not name renders as `unknown layer` rather
 * than being dropped — an unexplained value is worse than an oddly labelled
 * one in a view whose whole purpose is explanation.
 */
export function describeSource(entry: ParamProvenance, ctx: SourceContext): string {
  const floor = ctx.isReasoning ? 'reasoning floor' : 'default floor';

  switch (entry.kind) {
    case 'layer':
      switch (entry.layer) {
        case 'request':
          return 'request parameters';
        case 'profile':
          return ctx.profile ? `profile '${ctx.profile}'` : 'profile';
        case 'modelUserSet':
          return 'per-model defaults (user-set)';
        case 'global':
          return 'global settings';
        case 'modelAutoDetected':
          // One rung, three possible sources — see `SourceContext.defaultsOrigin`.
          if (ctx.defaultsOrigin === 'published') {
            return 'per-model defaults (published by the model author)';
          }
          if (ctx.defaultsOrigin === 'measured') {
            return 'per-model defaults (measured by a tune sweep)';
          }
          return 'per-model defaults (auto-detected: reasoning tag)';
        default:
          return 'unknown layer';
      }
    case 'floor':
      return floor;
    case 'floorCoupled':
      return `${floor} (coupled to temperature layer)`;
    case 'unset':
      return 'unset by design';
    case 'suppressedByTemplate':
      return 'suppressed (this template does not read it)';
    default:
      return UNKNOWN;
  }
}

/**
 * Format one resolved value for display.
 *
 * Whole floats keep a decimal place, as they do on the CLI: `1.0` reads as a
 * sampling parameter where a bare `1` reads as a count.
 */
export function formatParamValue(
  param: SamplingParamKey,
  value: number | null | undefined,
): string {
  if (value == null) return UNKNOWN;
  // Integer-valued parameters render bare. Two of them can legitimately be
  // negative — `dryPenaltyLastN`'s -1 (whole context) and
  // `reasoningBudgetTokens`' -1 (defer to the launch default) — which
  // `toFixed(1)` would render as a misleading "-1.0".
  if (
    param === 'topK' ||
    param === 'dryAllowedLength' ||
    param === 'dryPenaltyLastN' ||
    param === 'reasoningBudgetTokens'
  ) {
    return String(value);
  }
  if (param === 'maxTokens') return value.toLocaleString();
  return Number.isInteger(value) ? value.toFixed(1) : String(value);
}

/**
 * Read the value for one provenance entry out of the resolved config.
 *
 * `number | null` and not `| undefined`: `resolved` is the wire's
 * `InferenceConfig`, where all eighteen keys are present and an unset
 * parameter arrives as `null`. There is no absent case to represent.
 */
export function resolvedValue(
  resolved: InferenceConfig,
  param: SamplingParamKey,
): number | null {
  return resolved[param];
}

/**
 * Render one explain-table row's resolved value.
 *
 * Exists because `reasoningEffort` is the one attributed field whose value is
 * not a number: `formatParamValue` would have fallen through to
 * `Number.isInteger('high')` and printed the level by accident, which is the
 * right output for the wrong reason and would have become the wrong output the
 * moment the numeric branches were touched.
 */
export function formatProvenanceValue(
  param: ProvenanceParamKey,
  resolved: InferenceConfig,
): string {
  if (param === 'reasoningEffort') return resolved.reasoningEffort ?? UNKNOWN;
  return formatParamValue(param, resolvedValue(resolved, param));
}

/**
 * Trim a value that made a round trip through `f32` and JSON.
 *
 * gglib's own numbers are `f32`, so a resolved `0.7` arrives as
 * `0.699999988079071`. Rendering that verbatim makes an ordinary override look
 * like a defect. `ProxySamplingPanel.formatValue` and the CLI's
 * `fmt_published` do the same thing to the same values.
 */
function trimFloat(value: number | null | undefined): string {
  // Tolerates a missing number rather than throwing. The types say it cannot
  // happen — `PublishedDefaultDto`'s arms carry `published` where they claim
  // to — but this renders inside the inspector panel, and a `TypeError` here
  // unmounts the whole panel rather than spoiling one row. It reads `—`,
  // never `0`: a fabricated zero would render as a number the model author
  // published, which is the one thing this function must not invent.
  if (value == null) return UNKNOWN;
  return Number(value.toPrecision(6)).toString();
}

/**
 * Describe what the model's own GGUF published for one parameter.
 *
 * `null` when the model published nothing for it, which is the case for almost
 * every model and every parameter — `presencePenalty` and `dryMultiplier` have
 * no GGUF key at all, so they can never appear here.
 *
 * The wording matches `explain_display.rs`'s notes so the two surfaces describe
 * the same fact the same way.
 */
export function describePublished(entry: PublishedDefault): string {
  // The four `?? 0` guards are gone and are not coming back:
  // `PublishedDefaultDto` is a discriminated union, so `published` is present
  // on the three arms that read it and absent from `unreadable`. The
  // hand-written mirror was a flat object that made both numbers optional on
  // every state, and `?? 0` was standing in for a shape the wire does not
  // have — at the cost of printing a `0` the model author never published.
  //
  // The `default:` arm does stay, on the argument `publishedByParam` makes
  // below: the declaration is a claim about the endpoint, not a guarantee
  // about the bytes, and without it an unrecognised `state` returns
  // `undefined` from a function typed `string`.
  switch (entry.state) {
    case 'overridden':
      return `${entry.key} = ${trimFloat(entry.published)}; gglib is sending ${trimFloat(
        entry.sending,
      )}`;
    // Named even though it is benign: the row above reads as unset, and an
    // unset row with no note is indistinguishable from a gap. The missing
    // number is the model author's, not nobody's.
    case 'deferred':
      return `${entry.key} = ${trimFloat(entry.published)}; gglib defers to it`;
    case 'restated':
      return `${entry.key} = ${trimFloat(entry.published)}; gglib sends the same value`;
    case 'unreadable':
      return `${entry.key} is set to a value gglib cannot read`;
    default:
      return UNKNOWN;
  }
}

/**
 * Index published entries by parameter, for rendering beside their rows.
 *
 * Still tolerates the field being absent, though `SamplingExplanation` now
 * types it as an unconditional array. The value reaches this function through
 * an unchecked cast over parsed JSON, so the declaration is a claim about the
 * endpoint rather than a guarantee about the bytes — and "this model published
 * nothing" is the right reading of a missing list either way.
 */
export function publishedByParam(
  published: PublishedDefault[] | undefined,
): Map<ProvenanceParamKey, PublishedDefault> {
  return new Map((published ?? []).map((entry) => [entry.param, entry]));
}

/**
 * The client's own fields that survive an untrusted request.
 *
 * A hand-kept mirror of `CLIENT_AUTHORITATIVE_KEYS` in
 * `crates/gglib-core/src/request_pipeline/sampling.rs` — TypeScript cannot
 * read a Rust constant, and this list is what the inspector tells the operator
 * the trust gate lets through. It said `max_tokens` alone for a gate that had
 * already gone client-authoritative on `reasoning_budget_tokens`, so the
 * inspector described a boundary that no longer existed. Named and pinned by
 * `caveats name every client-authoritative key` rather than inlined in the
 * sentence, so the next addition is a one-line edit in an obvious place.
 *
 * The rule for what belongs here is Rust-side: budgets, not tastes — a key
 * that says what the request *is* rather than how it should sample.
 */
export const CLIENT_AUTHORITATIVE_KEYS = ['max_tokens', 'reasoning_budget_tokens'] as const;

/**
 * The two rungs the table cannot show, because neither is stored on the model.
 *
 * Without these the view looks complete while omitting the layers that
 * actually outrank everything in it.
 *
 * The untrusted note is built from {@link CLIENT_AUTHORITATIVE_KEYS} for the
 * same reason the CLI's is built from the Rust constant: a sentence that
 * spells the carve-out out is a second copy of it, and the two drifted the
 * first time the carve-out grew.
 */
export function caveats(trustClientSampling: boolean): [string, string] {
  return [
    'Operator flags (gglib proxy --temperature, ...) outrank every layer above.',
    trustClientSampling
      ? 'Client sampling is trusted and outranks all but those flags.'
      : `Client sampling is ignored, except ${CLIENT_AUTHORITATIVE_KEYS.join(', ')}.`,
  ];
}
