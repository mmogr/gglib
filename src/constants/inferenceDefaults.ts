/**
 * Fallback values and accepted ranges for the sampling parameters.
 *
 * The authoritative values live in Rust: `InferenceConfig::with_hardcoded_defaults()`
 * in `crates/gglib-core/src/domain/inference.rs` supplies the floor, and
 * `validate_inference_config()` in `crates/gglib-core/src/settings.rs` decides
 * what a save will accept. These entries mirror both so the GUI can state the
 * fallback and refuse out-of-range input before a round trip.
 *
 * As with `settingsDefaults.ts`, mirroring is not enforcement — but here it is
 * checked: `tests/ts/contracts/settingsBounds.test.ts` reads those two Rust
 * functions and fails if a range here is wider than what the backend accepts,
 * or if a stated default is not the real one.
 *
 * `min`/`max` are frequently *narrower* than Rust's. Rust puts no ceiling on
 * Top K or Max Tokens at all; the caps here are UI guard rails and the contract
 * test permits them, since a subset can only reject input the backend would
 * have rejected anyway.
 *
 * Note these are the *floor* — the bottom rung of the resolution ladder
 * (request → profile → per-model → global settings → floor). Only the global
 * settings surface sits directly above it, so only there is the floor what an
 * empty field falls through to. Every other surface has to ask the backend
 * what a parameter actually resolves to.
 */

import type { InferenceConfig, SamplingParamKey } from '../types';

/** Fallback and bounds for one sampling parameter. */
export interface InferenceParamSpec {
  /**
   * Value the floor supplies when no layer sets one, or `null` when the floor
   * deliberately supplies nothing — see `maxTokens`.
   */
  readonly default: number | null;
  readonly min: number;
  readonly max: number;
  readonly step: number;
}

export const INFERENCE_PARAMS: Record<SamplingParamKey, InferenceParamSpec> = {
  /** Rust: `with_hardcoded_defaults().temperature`, range `0.0..=2.0`. */
  temperature: { default: 0.7, min: 0, max: 2, step: 0.05 },

  /**
   * Rust: unset at the floor. ADR 0003 measured gglib's old `0.95` to be
   * exactly llama.cpp's own default on the pinned build, so restating it was a
   * redundant assertion that would silently override whatever upstream chooses
   * next. Deferred; llama.cpp supplies 0.95. Range `0.0..=1.0`.
   */
  topP: { default: null, min: 0, max: 1, step: 0.05 },

  /**
   * Rust: unset at the floor, deferred to llama.cpp's own 40 (ADR 0003).
   * Validation only requires `> 0`.
   */
  topK: { default: null, min: 1, max: 200, step: 1 },

  /**
   * Rust: `with_hardcoded_defaults()` sets `max_tokens: None`, deliberately —
   * resolution force-writes every `Some` field into the outgoing request, so a
   * fallback here would cap *every* request that did not name its own.
   * Unset, llama-server applies `n_predict = -1` and generates until a stop
   * token or the context limit.
   *
   * So there is no default to state, and the UI must not invent one.
   * Validation only requires `!= 0`.
   */
  maxTokens: { default: null, min: 1, max: 8192, step: 1 },

  /**
   * Rust: unset at the floor, deferred to llama.cpp's own 1.0 (ADR 0003).
   * Validation requires `> 0.0`, so the minimum is one step above zero rather
   * than zero. Nothing else here has an exclusive bound.
   */
  repeatPenalty: { default: null, min: 0.05, max: 2, step: 0.05 },

  /**
   * Rust: unset at the neutral floor, deferred to llama.cpp's own 0.0
   * (ADR 0003). Range `0.0..=2.0`.
   *
   * `reasoning`-tagged models use `reasoning_floor()` instead, which *does*
   * assert `1.0` — class-aware policy llama.cpp has no notion of. So this is
   * one of the two parameters gglib asserts for one model class and defers for
   * every other (see `minP`); `default: null` describes the neutral case only.
   */
  presencePenalty: { default: null, min: 0, max: 2, step: 0.05 },

  /**
   * Rust: unset at the neutral floor, deferred to llama.cpp's own 0.05
   * (ADR 0003). Range `0.0..=1.0`.
   *
   * `reasoning`-tagged models use `reasoning_floor()` instead, which asserts
   * `0.0` per Qwen3.6's guidance to disable min-p — a measured divergence from
   * upstream, so it stays force-written while the neutral case defers.
   */
  minP: { default: null, min: 0, max: 1, step: 0.01 },

  /**
   * Rust: unset at the floor; llama.cpp's own default is 0.0 (disabled).
   * Modelled after ADR 0003 so the untrusted-client gate governs it — a
   * standard OpenAI field that previously passed through ungoverned.
   * Validated `-2.0..=2.0`, the OpenAI-spec range; negative values encourage
   * reuse and are valid upstream.
   */
  frequencyPenalty: { default: null, min: -2, max: 2, step: 0.05 },

  /**
   * Rust: unset at the floor, deferred to llama.cpp's own 0.0 (disabled) —
   * introduced after ADR 0003, so it was never floored at all. Validation
   * requires non-negative; the `max` here is a UI guard rail (useful ranges
   * sit at or below the base temperature).
   */
  dynatempRange: { default: null, min: 0, max: 2, step: 0.05 },

  /**
   * Rust: unset at the floor; llama.cpp's own default is 1.0. Inert unless
   * `dynatempRange` is set. Validation requires a positive value, so the
   * minimum is one step above zero.
   */
  dynatempExponent: { default: null, min: 0.05, max: 5, step: 0.05 },

  /**
   * Rust: unset at the floor; llama.cpp's own default is -1.0 (disabled, and
   * any value at or below zero reads as off). Validation accepts -1.0 or
   * greater, which is why `min` is -1. The paper's evaluated range tops out
   * near 3; 5 is the UI guard rail.
   */
  topNSigma: { default: null, min: -1, max: 5, step: 0.05 },

  /**
   * Rust: unset at the floor, deferred to llama.cpp's own 0.0 (ADR 0003), so
   * DRY stays off by silence rather than by gglib restating the zero upstream
   * already defaults to. Turning it on for every untuned model would be a
   * tuning decision, not a default. Validated `0.0..=5.0`; `max` here is a UI
   * guard rail well below that, since useful values sit under 1.0.
   */
  dryMultiplier: { default: null, min: 0, max: 2, step: 0.05 },

  /**
   * Rust: `with_hardcoded_defaults()` leaves `dry_base` unset, so llama.cpp's
   * own default (1.75) applies. Validation requires `> 1.0`, so the minimum is
   * one step above it.
   */
  dryBase: { default: null, min: 1.05, max: 4, step: 0.05 },

  /**
   * Rust: unset at the floor; llama.cpp defaults to 2. Validation only
   * requires non-negative.
   */
  dryAllowedLength: { default: null, min: 0, max: 20, step: 1 },

  /**
   * Rust: unset at the floor; llama.cpp defaults to 64 — measured against the
   * pinned build, see `scripts/experiments/sampler_wire_semantics.py`. This
   * comment previously said -1, which is a legal *value* meaning "scan the
   * whole context" but is not the default. Validation accepts -1 or
   * non-negative, which is why `min` is -1 rather than 0.
   */
  dryPenaltyLastN: { default: null, min: -1, max: 8192, step: 1 },

  /**
   * Rust: unset at the floor, and deliberately unfloorable — `-1` already means
   * "defer to the launch default", which emitting no key does for free.
   * Validation accepts `-1` or greater (exactly upstream's range: llama-server
   * answers `-2` with an HTTP 400 naming it), which is why `min` is -1.
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
  reasoningBudgetTokens: { default: null, min: -1, max: 32768, step: 1 },
};

/**
 * Every field of the wire's `InferenceConfig`, in one runtime list.
 *
 * A superset of `INFERENCE_PARAMS`' keys, which is why it is a separate
 * constant rather than `Object.keys` over that table: `INFERENCE_PARAMS`
 * describes only the parameters with a checkable numeric range, and two
 * fields have none — `reasoningEffort` is an enum, and `seed` is an arbitrary
 * u32 with no floor worth publishing. Both still cross the wire.
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
