/**
 * Display helpers for the resolved-sampling section.
 *
 * The wording mirrors `gglib model explain`
 * (`crates/gglib-cli/src/presentation/explain_display.rs`) so the two surfaces
 * describe the same fact the same way. Kept apart from the component so each
 * mapping is directly testable.
 */

import type { InferenceConfig, ParamProvenance, SamplingParamKey } from '../types';
import { UNKNOWN } from './format';

/** Human-readable label per parameter, in the order the server sends them. */
export const PARAM_LABELS: Record<SamplingParamKey, string> = {
  temperature: 'Temperature',
  topP: 'Top P',
  topK: 'Top K',
  presencePenalty: 'Presence Penalty',
  repeatPenalty: 'Repeat Penalty',
  minP: 'Min P',
  maxTokens: 'Max Tokens',
};

/** The facts about the model that change how a source reads. */
export interface SourceContext {
  /** The profile applied, if one was selected. */
  profile?: string | null;
  /** Selects which floor the coupled trio falls back to. */
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
  if (param === 'topK') return String(value);
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
