/**
 * Accepted ranges for the sampling parameters.
 *
 * The authoritative ranges live in Rust: `validate_inference_config()` decides
 * what a save will accept, against the constants in
 * `crates/gglib-core/src/settings_bounds.rs` that the entries below name.
 * These entries mirror them so the GUI can refuse out-of-range input before a
 * round trip.
 *
 * As with `settingsDefaults.ts`, mirroring is not enforcement — but here it is
 * checked: `tests/ts/contracts/settingsBounds.test.ts` reads
 * `contracts/settings/bounds.json`, which the Rust tests write, and fails if a
 * range here is wider than what the backend accepts.
 *
 * `min`/`max` are frequently *narrower* than Rust's. Rust puts no ceiling on
 * Top K or Max Tokens at all; the caps here are UI guard rails and the contract
 * test permits them, since a subset can only reject input the backend would
 * have rejected anyway.
 *
 * There is no fallback value here, because there is no floor beneath the
 * resolution ladder (ADR 0022): a field no layer names is not sent, and
 * llama.cpp's own default applies. Each entry's comment names that default.
 */

import type { InferenceConfig, SamplingParamKey } from '../types';

/** Bounds for one sampling parameter. */
export interface InferenceParamSpec {
  readonly min: number;
  readonly max: number;
  readonly step: number;
}

export const INFERENCE_PARAMS: Record<SamplingParamKey, InferenceParamSpec> = {
  /**
   * Unset, llama.cpp's own 0.8 applies (ADR 0022 deferred gglib's 0.7).
   * Range `TEMPERATURE_RANGE`.
   */
  temperature: { min: 0, max: 2, step: 0.05 },

  /** Unset, llama.cpp's own 0.95 applies (ADR 0003). Range `TOP_P_RANGE`. */
  topP: { min: 0, max: 1, step: 0.05 },

  /**
   * Unset, llama.cpp's own 40 applies (ADR 0003). Validation only requires
   * `TOP_K_MIN` or more.
   */
  topK: { min: 1, max: 200, step: 1 },

  /**
   * Unset, no `max_tokens` key is sent and llama-server applies
   * `n_predict = -1`, generating until a stop token or the context limit.
   * Validation only requires `MAX_TOKENS_MIN` or more; `max` is a UI guard
   * rail, as high as the `reasoning` recipe's own 32768.
   */
  maxTokens: { min: 1, max: 32768, step: 1 },

  /**
   * Unset, llama.cpp's own 1.0 applies (ADR 0003). Validation requires more
   * than `REPEAT_PENALTY_EXCLUSIVE_MIN`, so the minimum is one step above it
   * rather than on it.
   */
  repeatPenalty: { min: 0.05, max: 2, step: 0.05 },

  /**
   * Unset, llama.cpp's own 0.0 applies (ADR 0003, ADR 0022).
   * Range `PRESENCE_PENALTY_RANGE`.
   */
  presencePenalty: { min: 0, max: 2, step: 0.05 },

  /**
   * Unset, llama.cpp's own 0.05 applies (ADR 0003, ADR 0022).
   * Range `MIN_P_RANGE`.
   */
  minP: { min: 0, max: 1, step: 0.01 },

  /**
   * Unset, llama.cpp's own 0.0 (disabled) applies. Modelled after ADR 0003 so
   * the untrusted-client gate governs it — a standard OpenAI field that
   * previously passed through ungoverned. Validated against
   * `FREQUENCY_PENALTY_RANGE`, the OpenAI-spec range; negative values
   * encourage reuse and are valid upstream.
   */
  frequencyPenalty: { min: -2, max: 2, step: 0.05 },

  /**
   * Unset, llama.cpp's own 0.0 (disabled) applies. Validation requires
   * `DYNATEMP_RANGE_MIN` or more; the `max` here is a UI guard rail (useful
   * ranges sit at or below the base temperature).
   */
  dynatempRange: { min: 0, max: 2, step: 0.05 },

  /**
   * Unset, llama.cpp's own 1.0 applies. Inert unless `dynatempRange` is set.
   * Validation requires more than `DYNATEMP_EXPONENT_EXCLUSIVE_MIN`, so the
   * minimum is one step above it.
   */
  dynatempExponent: { min: 0.05, max: 5, step: 0.05 },

  /**
   * Unset, llama.cpp's own -1.0 (disabled, and any value at or below zero
   * reads as off) applies. Validation accepts `TOP_N_SIGMA_MIN` or greater,
   * which is why `min` is -1. The paper's evaluated range tops out near 3; 5
   * is the UI guard rail.
   */
  topNSigma: { min: -1, max: 5, step: 0.05 },

  /**
   * Unset, llama.cpp's own 0.0 applies (ADR 0003), so DRY stays off by
   * silence. Turning it on for every untuned model would be a tuning
   * decision, not a default. Validated against `DRY_MULTIPLIER_RANGE`; `max`
   * here is a UI guard rail well below its end, since useful values sit under
   * 1.0.
   */
  dryMultiplier: { min: 0, max: 2, step: 0.05 },

  /**
   * Unset, llama.cpp's own 1.75 applies. Validation requires more than
   * `DRY_BASE_EXCLUSIVE_MIN`, so the minimum is one step above it.
   */
  dryBase: { min: 1.05, max: 4, step: 0.05 },

  /**
   * Unset, llama.cpp's own 2 applies. Validation only requires
   * `DRY_ALLOWED_LENGTH_MIN` or more.
   */
  dryAllowedLength: { min: 0, max: 20, step: 1 },

  /**
   * Unset, llama.cpp's own 64 applies — measured against the pinned build,
   * see `scripts/experiments/sampler_wire_semantics.py`. -1 is a legal
   * *value* meaning "scan the whole context" but is not the default.
   * Validation accepts `DRY_PENALTY_LAST_N_MIN` or greater, which is why
   * `min` is -1 rather than 0.
   */
  dryPenaltyLastN: { min: -1, max: 8192, step: 1 },

  /**
   * Unset, the launch default applies — `-1` already means "defer to the
   * launch default", which emitting no key does for free. Validation accepts
   * `REASONING_BUDGET_TOKENS_MIN` or greater (exactly upstream's range:
   * llama-server answers `-2` with an HTTP 400 naming it), which is why `min`
   * is -1.
   *
   * `max` is a UI guard rail with no Rust counterpart — the backend's ceiling
   * is `i32::MAX`. 32768 is the top rung of the CLI's own reasoning ladder, so
   * the field offers the range the rest of gglib recommends rather than a range
   * no context window can hold.
   *
   * Unlike every other entry here this one is not a *sampling* parameter, and
   * it is here for a narrow reason: it is a number with a real backend-checked
   * range, so `settingsBounds.test.ts` can hold the transcription honest. Its
   * twin, `reasoningEffort`, is an enum and stays out of this table entirely.
   */
  reasoningBudgetTokens: { min: -1, max: 32768, step: 1 },
};

/**
 * Every field of the wire's `InferenceConfig`, in one runtime list.
 *
 * A superset of `INFERENCE_PARAMS`' keys, which is why it is a separate
 * constant rather than `Object.keys` over that table: `INFERENCE_PARAMS`
 * describes only the parameters with a checkable numeric range, and two
 * fields have none — `reasoningEffort` is an enum, and `seed` is an arbitrary
 * u32 with no range worth publishing. Both still cross the wire.
 *
 * The `satisfies` pins every entry to a real field, and the assertion below
 * pins the list to *all* of them: adding a field in Rust regenerates
 * `InferenceConfig`, and this fails to compile until it is listed here. That
 * is the whole point — the nine fields the serve request used to drop were
 * dropped because a hand-kept list had no way to notice them.
 */
export const INFERENCE_CONFIG_KEYS = [
  'temperature',
  'topP',
  'topK',
  'maxTokens',
  'repeatPenalty',
  'presencePenalty',
  'minP',
  'frequencyPenalty',
  'dynatempRange',
  'dynatempExponent',
  'topNSigma',
  'dryMultiplier',
  'dryBase',
  'dryAllowedLength',
  'dryPenaltyLastN',
  'reasoningEffort',
  'reasoningBudgetTokens',
  'seed',
] as const satisfies readonly (keyof InferenceConfig)[];

type AssertNoUnlistedFields<T extends never> = T;
/** Fails to compile if `InferenceConfig` grows a field this list omits. */
export type InferenceConfigKeysAreComplete = AssertNoUnlistedFields<
  Exclude<keyof InferenceConfig, (typeof INFERENCE_CONFIG_KEYS)[number]>
>;
