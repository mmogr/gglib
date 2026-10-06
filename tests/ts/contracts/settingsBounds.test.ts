/**
 * Contract test: the GUI's numbers against the Rust ones they mirror.
 *
 * `src/constants/settingsDefaults.ts` and `src/constants/inferenceDefaults.ts`
 * both transcribe values that actually live in Rust. Transcription drifts, and
 * when it does the GUI misinforms the user quietly: it offers a default the
 * backend does not have, or accepts input the backend will reject on save.
 * Both had happened by the time this test was written.
 *
 * The Rust side of the comparison is `contracts/settings/bounds.json`.
 * `gglib-core`'s settings-bounds tests write it from the constants
 * `validate_settings` and `validate_inference_config` check against, and from
 * what `Settings::with_defaults()` and the two inference floors return, and
 * fail there when it goes stale. So a bound or a default moved in Rust moves
 * the file, and this holds the GUI's copies to the file:
 *
 *   1. The GUI never accepts what the backend rejects — every `[min, max]` is a
 *      subset of the accepted range. A subset, not an equality: several GUI
 *      caps are deliberate guard rails over a Rust bound that does not exist
 *      (Top K, Max Tokens, Max Tool Iterations), and a narrower field can only
 *      reject input that would have failed validation anyway.
 *   2. A stated default is the real one — and a value Rust deliberately leaves
 *      unset is not given an invented default.
 *
 * Nothing here reads Rust source, so the validators may be laid out however
 * they read best. A field the file does not name throws rather than passing
 * unchecked, so dropping one from the file turns this red instead of silently
 * retiring the guarantee.
 */

import { describe, it, expect } from 'vitest';

import * as settingsDefaults from '../../../src/constants/settingsDefaults';
import { INFERENCE_PARAMS } from '../../../src/constants/inferenceDefaults';
import type { SamplingParamKey } from '../../../src/types';

import { rust } from './rustSource';

/**
 * What a validator accepts for one number: `min` and up, or anything greater
 * than `above`, and no further than `max` where there is a ceiling.
 */
interface Accepted {
  min?: number;
  above?: number;
  max?: number;
}

const BOUNDS = JSON.parse(rust('contracts/settings/bounds.json')) as {
  /** By `Settings` field: what `validate_settings` holds it to. */
  settings: Record<string, Accepted>;
  /** By `Settings` field: what `Settings::with_defaults()` sets. */
  settings_defaults: Record<string, number | null>;
  /** By `InferenceConfig` wire key: what `validate_inference_config` holds it to. */
  inference: Record<string, Accepted>;
  /** `InferenceConfig::with_hardcoded_defaults()`, as it serialises. */
  inference_floor: Record<string, unknown>;
  /** `InferenceConfig::reasoning_floor()`, as it serialises. */
  reasoning_floor: Record<string, unknown>;
};

/** One entry of the file, or a throw naming what it lacks. */
function recorded<T>(section: keyof typeof BOUNDS, key: string): T {
  const entries = BOUNDS[section] as Record<string, unknown>;
  if (!(key in entries)) {
    throw new Error(`contracts/settings/bounds.json has no ${section} entry for ${key}`);
  }
  return entries[key] as T;
}

/** The GUI's `[min, max]` lies inside what the backend accepts. */
function expectOffersOnlyAccepted(offered: { min: number; max: number }, accepted: Accepted) {
  if (accepted.above !== undefined) {
    // The floor itself is refused, so a field starting exactly on it is out of
    // bounds.
    expect(offered.min).toBeGreaterThan(accepted.above);
  } else if (accepted.min !== undefined) {
    expect(offered.min).toBeGreaterThanOrEqual(accepted.min);
  } else {
    throw new Error(`a recorded range has neither a min nor an above: ${JSON.stringify(accepted)}`);
  }

  expect(offered.max).toBeLessThanOrEqual(accepted.max ?? Infinity);
}

// ── The settings modal's numeric fields ─────────────────────────────────────

const SETTINGS_FIELDS: {
  label: string;
  spec: settingsDefaults.NumericSettingSpec;
  rustField: string;
}[] = [
  { label: 'Proxy Server Port', spec: settingsDefaults.PROXY_PORT, rustField: 'proxy_port' },
  { label: 'Base Server Port', spec: settingsDefaults.LLAMA_BASE_PORT, rustField: 'llama_base_port' },
  {
    label: 'Max Download Queue Size',
    spec: settingsDefaults.MAX_DOWNLOAD_QUEUE_SIZE,
    rustField: 'max_download_queue_size',
  },
  { label: 'Default Context Size', spec: settingsDefaults.CONTEXT_SIZE, rustField: 'default_context_size' },
];

/** A spec's default as the file writes one: a number, or null for unset. */
const statedDefault = (spec: settingsDefaults.NumericSettingSpec) =>
  spec.default === null ? null : Number(spec.default);

describe('settings fields vs validate_settings', () => {
  it.each(SETTINGS_FIELDS)('$label offers only values the backend accepts', ({ spec, rustField }) => {
    expectOffersOnlyAccepted(
      { min: Number(spec.min), max: Number(spec.max) },
      recorded<Accepted>('settings', rustField),
    );
  });

  it.each(SETTINGS_FIELDS)(
    '$label states the default Settings::with_defaults() actually uses',
    ({ spec, rustField }) => {
      expect(statedDefault(spec)).toBe(recorded<number | null>('settings_defaults', rustField));
    },
  );

  it('Max Tool Iterations tracks the agent default, and caps only in the UI', () => {
    // No range in the file: `validate_settings` does not bound it, which
    // `gglib-core`'s settings-bounds tests prove by storing its extremes. The
    // 1-50 range is a UI guard rail, which is why this one is asserted apart
    // from the table above.
    expect(BOUNDS.settings).not.toHaveProperty('max_tool_iterations');

    expect(statedDefault(settingsDefaults.MAX_TOOL_ITERATIONS)).toBe(
      recorded<number | null>('settings_defaults', 'max_tool_iterations'),
    );
  });
});

// ── The sampling parameters ─────────────────────────────────────────────────

/**
 * Every key of `INFERENCE_PARAMS`, which its `Record<SamplingParamKey, …>`
 * type keeps complete. They are the wire's own camelCase names, and the file
 * is keyed by those.
 *
 * `reasoningBudgetTokens` is among them though it is not a sampling
 * parameter: it is the one reasoning control with a number and a Rust-side
 * range, which is exactly what this table checks. Its twin `reasoningEffort`
 * is an enum with no bounds and no numeric floor, so it is absent from
 * `SamplingParamKey`.
 */
const PARAMS = Object.keys(INFERENCE_PARAMS) as SamplingParamKey[];

describe('sampling parameters vs validate_inference_config', () => {
  it.each(PARAMS)('%s offers only values the backend accepts', (param) => {
    expectOffersOnlyAccepted(INFERENCE_PARAMS[param], recorded<Accepted>('inference', param));
  });

  it.each(PARAMS)('%s states the floor with_hardcoded_defaults() actually uses', (param) => {
    // null on both sides is the point for max_tokens: Rust leaves it unset
    // deliberately, so the GUI must not invent one.
    expect(INFERENCE_PARAMS[param].default).toBe(recorded<number | null>('inference_floor', param));
  });

  it('only presence_penalty and min_p differ in the reasoning floor', () => {
    // inferenceDefaults.ts annotates presencePenalty and minP as the two
    // model-dependent floors. If reasoning_floor() ever overrides a third
    // field, those comments — and the settings-surface captions built on
    // them — go stale.
    const differing = Object.keys(BOUNDS.reasoning_floor).filter(
      (key) => BOUNDS.reasoning_floor[key] !== BOUNDS.inference_floor[key],
    );

    expect(differing.sort()).toEqual(['minP', 'presencePenalty']);
  });
});
