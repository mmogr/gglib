/**
 * A complete `InferenceConfig`, as the wire actually sends one.
 *
 * Every field is present and `null` where nothing is set — `gglib-core`'s
 * `InferenceConfig` carries no `skip_serializing_if`, so `GET /explain` and
 * every `inferenceDefaults` payload has always looked like this. A fixture
 * naming two fields was describing a response no endpoint produces, and only
 * typechecked because the hand-written mirror made all eighteen optional.
 *
 * Use for *response* fixtures. A form's in-progress state is a
 * `SparseInferenceConfig` and should stay a bare object literal.
 */
import type { InferenceConfig } from '../../../src/types';

const UNSET: InferenceConfig = {
  temperature: null,
  topP: null,
  topK: null,
  maxTokens: null,
  repeatPenalty: null,
  presencePenalty: null,
  minP: null,
  frequencyPenalty: null,
  dynatempRange: null,
  dynatempExponent: null,
  topNSigma: null,
  dryMultiplier: null,
  dryBase: null,
  dryAllowedLength: null,
  dryPenaltyLastN: null,
  reasoningEffort: null,
  reasoningBudgetTokens: null,
  seed: null,
};

/** A resolved config with `overrides` applied over "nothing set". */
export function resolvedConfig(overrides: Partial<InferenceConfig> = {}): InferenceConfig {
  return { ...UNSET, ...overrides };
}
