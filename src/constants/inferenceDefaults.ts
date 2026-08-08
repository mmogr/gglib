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

import type { SamplingParamKey } from '../types';

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

  /** Rust: `with_hardcoded_defaults().top_p`, range `0.0..=1.0`. */
  topP: { default: 0.95, min: 0, max: 1, step: 0.05 },

  /** Rust: `with_hardcoded_defaults().top_k`; validation only requires `> 0`. */
  topK: { default: 40, min: 1, max: 200, step: 1 },

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
   * Rust: `with_hardcoded_defaults().repeat_penalty`; validation requires
   * `> 0.0`, so the minimum is one step above zero rather than zero. Nothing
   * else here has an exclusive bound.
   */
  repeatPenalty: { default: 1.0, min: 0.05, max: 2, step: 0.05 },

  /**
   * Rust: `with_hardcoded_defaults().presence_penalty`, range `0.0..=2.0`.
   *
   * `reasoning`-tagged models use `reasoning_floor()` instead, which overrides
   * this to `1.0` — one of the two parameters whose floor is model-dependent
   * (see `minP` for the other).
   */
  presencePenalty: { default: 0.0, min: 0, max: 2, step: 0.05 },

  /**
   * Rust: `with_hardcoded_defaults().min_p`, range `0.0..=1.0`.
   *
   * `0.05` matches llama.cpp's own default. `reasoning`-tagged models use
   * `reasoning_floor()` instead, which overrides this to `0.0` per Qwen3.6's
   * guidance to disable min-p.
   */
  minP: { default: 0.05, min: 0, max: 1, step: 0.01 },

  /**
   * Rust: `with_hardcoded_defaults().dry_multiplier`, validated `0.0..=5.0`.
   *
   * The floor states `0.0` — DRY expressible but off — because turning it on
   * for every untuned model is a tuning decision, not a default. `max` here is
   * a UI guard rail well below the validated ceiling; useful values sit under
   * 1.0.
   */
  dryMultiplier: { default: 0.0, min: 0, max: 2, step: 0.05 },

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
   * Rust: unset at the floor; llama.cpp defaults to -1, meaning scan the whole
   * context. Validation accepts -1 or non-negative, which is why `min` is -1
   * rather than 0.
   */
  dryPenaltyLastN: { default: null, min: -1, max: 8192, step: 1 },
};
