/**
 * Display helpers for the resolved-sampling section.
 *
 * The wording mirrors `gglib model explain`
 * (`crates/gglib-cli/src/presentation/explain_display.rs`) so the two surfaces
 * describe the same fact the same way. Kept apart from the component so each
 * mapping is directly testable.
 */

import type {
  InferenceConfig,
  ParamProvenance,
  PublishedDefault,
  SamplingParamKey,
} from '../types';
import { UNKNOWN } from './format';

/** Human-readable label per parameter, in the order the server sends them. */
export const PARAM_LABELS: Record<SamplingParamKey, string> = {
  temperature: 'Temperature',
  topP: 'Top P',
  topK: 'Top K',
  presencePenalty: 'Presence Penalty',
  repeatPenalty: 'Repeat Penalty',
  minP: 'Min P',
  dryMultiplier: 'DRY Multiplier',
  dryBase: 'DRY Base',
  dryAllowedLength: 'DRY Allowed Length',
  dryPenaltyLastN: 'DRY Penalty Last N',
  maxTokens: 'Max Tokens',
};

/** The facts about the model that change how a source reads. */
export interface SourceContext {
  /** The profile applied, if one was selected. */
  profile?: string | null;
  /** Selects which floor the coupled set falls back to. */
  isReasoning: boolean;
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
  // Integer-valued parameters render bare. `dryPenaltyLastN` is the only one
  // that can legitimately be negative (-1 = whole context), which `toFixed(1)`
  // would render as a misleading "-1.0".
  if (param === 'topK' || param === 'dryAllowedLength' || param === 'dryPenaltyLastN') {
    return String(value);
  }
  if (param === 'maxTokens') return value.toLocaleString();
  return Number.isInteger(value) ? value.toFixed(1) : String(value);
}

/** Read the value for one provenance entry out of the resolved config. */
export function resolvedValue(
  resolved: InferenceConfig,
  param: SamplingParamKey,
): number | undefined {
  return resolved[param];
}

/**
 * Trim a value that made a round trip through `f32` and JSON.
 *
 * gglib's own numbers are `f32`, so a resolved `0.7` arrives as
 * `0.699999988079071`. Rendering that verbatim makes an ordinary override look
 * like a defect. `ProxySamplingPanel.formatValue` and the CLI's
 * `fmt_published` do the same thing to the same values.
 */
function trimFloat(value: number): string {
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
  switch (entry.state) {
    case 'overridden':
      return `${entry.key} = ${trimFloat(entry.published ?? 0)}; gglib is sending ${trimFloat(
        entry.sending ?? 0,
      )}`;
    // Named even though it is benign: the row above reads as unset, and an
    // unset row with no note is indistinguishable from a gap. The missing
    // number is the model author's, not nobody's.
    case 'deferred':
      return `${entry.key} = ${trimFloat(entry.published ?? 0)}; gglib defers to it`;
    case 'restated':
      return `${entry.key} = ${trimFloat(entry.published ?? 0)}; gglib sends the same value`;
    case 'unreadable':
      return `${entry.key} is set to a value gglib cannot read`;
    default:
      return UNKNOWN;
  }
}

/**
 * Index published entries by parameter, for rendering beside their rows.
 *
 * Tolerates the field being absent: a backend that predates it sends nothing,
 * and that must read as "this model published nothing" rather than as an error.
 */
export function publishedByParam(
  published: PublishedDefault[] | undefined,
): Map<SamplingParamKey, PublishedDefault> {
  return new Map((published ?? []).map((entry) => [entry.param, entry]));
}

/**
 * The two rungs the table cannot show, because neither is stored on the model.
 *
 * Without these the view looks complete while omitting the layers that
 * actually outrank everything in it.
 */
export function caveats(trustClientSampling: boolean): [string, string] {
  return [
    'Operator flags (gglib proxy --temperature, ...) outrank every layer above.',
    trustClientSampling
      ? 'Client-supplied sampling is trusted and outranks all but those flags.'
      : 'Client-supplied sampling is ignored, except max_tokens.',
  ];
}
