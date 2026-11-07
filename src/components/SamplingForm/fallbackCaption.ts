/**
 * What to say under an inference field that has been left empty.
 *
 * The form renders at three rungs of the resolution ladder (request → profile
 * → per-model → global settings), and what an empty field falls through to
 * differs at each. The global-settings surface is not model-specific: beneath
 * it sit a model's recipe where it has one, and llama.cpp's own defaults
 * otherwise (there is no floor, ADR 0022). So the constant its caption names
 * is llama.cpp's number, and what a given model actually takes shows on the
 * `explain` surface.
 *
 * Everywhere else the answer comes from the backend. `resolve_layers_with_sources`
 * computes value and provenance in one pass precisely so nothing has to
 * re-derive them, and its docstring records that a second derivation had
 * already started explaining decisions the resolution did not take. So this
 * module formats what `GET /api/models/:id/explain` returns and never works
 * anything out for itself.
 */

import type { SamplingExplanation, SamplingLayerName, SamplingParamKey } from '../../types';
import { describeSource, formatParamValue } from '../../utils/samplingProvenance';

/** What sits below this surface in the ladder. */
export type InferenceFallback =
  /**
   * The global settings surface: beneath it, a model's recipe where it has
   * one, else llama.cpp's own defaults. The caption names llama.cpp's number;
   * a model's own answer shows on the `explain` surface.
   */
  | { kind: 'upstream' }
  /**
   * Other layers do. `ownLayer` is the rung this form edits, so a value the
   * explanation attributes to it can be recognised as one this form is in the
   * middle of changing.
   */
  | { kind: 'resolved'; ownLayer: SamplingLayerName; resolution: ResolutionState };

/** A `useSamplingExplanation` result, narrowed to what the caption needs. */
export interface ResolutionState {
  explanation: SamplingExplanation | null;
  isLoading: boolean;
  hasError: boolean;
}

/**
 * What an unset Max Tokens means for a chat: the agent loop gives each turn
 * that names none a limit of its own (`turn_limit.rs`), 32,768 for a
 * reasoning model and more for a thinking budget. A turn on a paired
 * machine's model gets none from this side.
 */
const CHAT_TURN_LIMIT = 'a chat turn gets at least 16,384 tokens';

/**
 * What to say for a parameter nothing names.
 *
 * "Unset" does not mean the same thing for all of them, so none of these can
 * be shared. Max Tokens unset means unbounded generation through the proxy,
 * and a turn's own limit in a chat ({@link CHAT_TURN_LIMIT}); an unset
 * sampler means llama.cpp's own default applies, which is a number worth
 * naming rather than an absence. A field reaching this map without an entry
 * gets no caption at all, which is the safe answer — better silence than Max
 * Tokens' wording under a threshold it does not describe.
 */
const UNSET_CAPTIONS: Partial<Record<SamplingParamKey, string>> = {
  maxTokens: `No limit through the proxy; ${CHAT_TURN_LIMIT}`,
  // gglib once restated some of these as a floor (ADR 0003, ADR 0022). The
  // number is still what applies, so the caption states it, as llama.cpp's.
  temperature: 'llama.cpp default: 0.8',
  topP: 'llama.cpp default: 0.95',
  topK: 'llama.cpp default: 40',
  repeatPenalty: 'llama.cpp default: 1.0',
  presencePenalty: 'llama.cpp default: 0.0',
  minP: 'llama.cpp default: 0.05',
  frequencyPenalty: 'llama.cpp default: 0.0',
  dynatempRange: 'llama.cpp default: 0.0 (dynatemp off)',
  dynatempExponent: 'llama.cpp default: 1.0',
  topNSigma: 'llama.cpp default: -1.0 (off)',
  dryMultiplier: 'llama.cpp default: 0.0 (DRY off)',
  dryBase: 'llama.cpp default: 1.75',
  dryAllowedLength: 'llama.cpp default: 2',
  dryPenaltyLastN: 'llama.cpp default: 64',
  // Not "llama.cpp default: N" like the rows above: there is no fixed number
  // to name. Unset means gglib emits no key, and what applies is whatever
  // `--reasoning-budget` the model was launched with — which is exactly what
  // the legal value -1 means, so leaving the field blank and typing -1 into it
  // do the same thing by different routes.
  reasoningBudgetTokens: 'Unset — the launch-time budget applies',
};

/** The provenance entry for one parameter, once it is safe to believe. */
function usableEntry(field: SamplingParamKey, fallback: InferenceFallback) {
  if (fallback.kind === 'upstream') return null;

  const { explanation, isLoading, hasError } = fallback.resolution;
  if (isLoading || hasError || !explanation) return null;

  const entry = explanation.sources.find((source) => source.param === field);
  if (!entry) return null;

  // The explanation describes what is *saved*. If this parameter's value came
  // from the layer this form edits, and the field is empty, the user has just
  // cleared it — the saved provenance describes a world that is one save out
  // of date, and the new answer is a rung further down than we can see.
  if (entry.kind === 'layer' && entry.layer === fallback.ownLayer) return null;

  return { entry, explanation };
}

/**
 * The number an empty field will actually take, or `null` if unknowable.
 *
 * Drives the placeholder and the slider thumb, so neither shows a number on a
 * surface where that number is not what applies — a slider parked at 0.70
 * beside a caption reading "Resolves to 1.20" would be its own kind of wrong.
 */
export function fallbackValue(
  field: SamplingParamKey,
  fallback: InferenceFallback,
): number | null {
  if (fallback.kind === 'upstream') return null;

  const usable = usableEntry(field, fallback);
  return usable ? (usable.explanation.resolved[field] ?? null) : null;
}

/**
 * The caption for one empty field, or `null` to say nothing.
 *
 * Saying nothing is a real answer here, used whenever a claim would be a
 * guess: mid-flight, and when the value on screen came from the very layer
 * this form edits (clearing it will re-resolve to something not yet known).
 */
export function fallbackCaption(
  field: SamplingParamKey,
  fallback: InferenceFallback,
): string | null {
  if (fallback.kind === 'upstream') return UNSET_CAPTIONS[field] ?? null;

  const { isLoading, hasError, explanation } = fallback.resolution;
  if (isLoading) return null;
  if (hasError || !explanation) return 'Resolution unavailable';

  const usable = usableEntry(field, fallback);
  if (!usable) return null;

  const source = describeSource(usable.entry, {
    profile: explanation.profile,
    // Three origins share the auto-detected rung, and this is the only thing
    // that separates them. Omitting it does not make the caption vaguer — it
    // makes it name the reasoning-tag guess for all three, which is this
    // module working something out for itself.
    defaultsOrigin: explanation.defaultsOrigin,
  });
  const value = explanation.resolved[field];

  if (value != null) return `Resolves to ${formatParamValue(field, value)} — ${source}`;
  // Only Max Tokens unset is a limit; any other empty field is just unset.
  return field === 'maxTokens'
    ? `No limit through the proxy; ${CHAT_TURN_LIMIT} — ${source}`
    : `Unset — ${source}`;
}
