/**
 * What to say under an inference field that has been left empty.
 *
 * The form renders at three rungs of the resolution ladder (request → profile
 * → per-model → global settings → floor), and what an empty field falls
 * through to differs at each. Only the global-settings surface sits directly
 * above the floor, so only there can the answer be a constant.
 *
 * Everywhere else the answer comes from the backend. `resolve_layers_with_sources`
 * computes value and provenance in one pass precisely so nothing has to
 * re-derive them, and its docstring records that a second derivation had
 * already started explaining decisions the resolution did not take. So this
 * module formats what `GET /api/models/:id/explain` returns and never works
 * anything out for itself.
 */

import type { SamplingExplanation, SamplingLayerName, SamplingParamKey } from '../../types';
import { INFERENCE_PARAMS } from '../../constants/inferenceDefaults';
import { describeSource, formatParamValue } from '../../utils/samplingProvenance';

/** What sits below this surface in the ladder. */
export type InferenceFallback =
  /** Nothing but the floor — the global settings surface. */
  | { kind: 'floor' }
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
 * What to say for a parameter the floor deliberately leaves unset.
 *
 * "Unset" does not mean the same thing for all of them, so none of these can
 * be shared. Max Tokens unset means genuinely unbounded generation; an unset
 * DRY parameter means llama.cpp's own default applies, which is a number worth
 * naming rather than an absence. A field reaching this map without an entry
 * gets no caption at all, which is the safe answer — better silence than Max
 * Tokens' wording under a threshold it does not describe.
 */
const UNSET_FLOOR_CAPTIONS: Partial<Record<SamplingParamKey, string>> = {
  maxTokens: 'No limit — generates until the context is full',
  // Deferred by ADR 0003: gglib's floor used to restate each of these and each
  // was measured equal to llama.cpp's own default on the pinned build. The
  // number is still what applies, so the caption still states it — what
  // changed is who supplies it, which is why these read "llama.cpp default"
  // rather than "Default".
  topP: 'llama.cpp default: 0.95',
  topK: 'llama.cpp default: 40',
  repeatPenalty: 'llama.cpp default: 1.0',
  presencePenalty: 'llama.cpp default: 0.0',
  minP: 'llama.cpp default: 0.05',
  dryMultiplier: 'llama.cpp default: 0.0 (DRY off)',
  dryBase: 'llama.cpp default: 1.75',
  dryAllowedLength: 'llama.cpp default: 2',
  dryPenaltyLastN: 'llama.cpp default: 64',
};

/**
 * Parameters whose floor depends on the model, and what `reasoning_floor()`
 * uses instead. This surface has no model to check against, so it names both.
 *
 * Since ADR 0003 these are the only two parameters gglib asserts for one model
 * class and defers for every other: a reasoning model is sent `min_p: 0.0`,
 * while everything else is sent no `min_p` at all and gets llama.cpp's 0.05.
 * That asymmetry is deliberate and invisible in the value alone, so the
 * caption has to carry it.
 */
const REASONING_FLOOR_OVERRIDES: Partial<Record<SamplingParamKey, string>> = {
  presencePenalty: '1.0',
  minP: '0.0',
};

/** Append the reasoning-model carve-out to a caption, when there is one. */
function withReasoningNote(field: SamplingParamKey, caption: string): string {
  const reasoning = REASONING_FLOOR_OVERRIDES[field];
  return reasoning ? `${caption} (${reasoning} for reasoning models)` : caption;
}

/** The provenance entry for one parameter, once it is safe to believe. */
function usableEntry(field: SamplingParamKey, fallback: InferenceFallback) {
  if (fallback.kind === 'floor') return null;

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
 * Drives the placeholder and the slider thumb, so neither shows the floor on a
 * surface where the floor is not what applies — a slider parked at 0.70 beside
 * a caption reading "Resolves to 1.20" would be its own kind of wrong.
 */
export function fallbackValue(
  field: SamplingParamKey,
  fallback: InferenceFallback,
): number | null {
  if (fallback.kind === 'floor') return INFERENCE_PARAMS[field].default;

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
  if (fallback.kind === 'floor') {
    const { default: floor } = INFERENCE_PARAMS[field];
    if (floor === null) {
      const deferred = UNSET_FLOOR_CAPTIONS[field];
      // The carve-out applies to the deferred branch too, and did not before:
      // once `presencePenalty` and `minP` left the neutral floor, returning
      // early here dropped the one note that explains why a reasoning model
      // behaves differently.
      return deferred ? withReasoningNote(field, deferred) : null;
    }

    return withReasoningNote(field, `Default: ${formatParamValue(field, floor)}`);
  }

  const { isLoading, hasError, explanation } = fallback.resolution;
  if (isLoading) return null;
  if (hasError || !explanation) return 'Resolution unavailable';

  const usable = usableEntry(field, fallback);
  if (!usable) return null;

  const source = describeSource(usable.entry, {
    profile: explanation.profile,
    isReasoning: explanation.isReasoning,
  });
  const value = explanation.resolved[field];

  return value == null
    ? `No limit — ${source}`
    : `Resolves to ${formatParamValue(field, value)} — ${source}`;
}
