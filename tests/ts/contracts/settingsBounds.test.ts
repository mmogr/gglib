/**
 * Contract test: the GUI's numbers against the Rust ones they mirror.
 *
 * `src/constants/settingsDefaults.ts` and `src/constants/inferenceDefaults.ts`
 * both transcribe values that actually live in Rust. Transcription drifts, and
 * when it does the GUI misinforms the user quietly: it offers a default the
 * backend does not have, or accepts input the backend will reject on save.
 * Both had happened by the time this test was written.
 *
 * So rather than trusting the comments, this reads the Rust source and checks
 * two invariants:
 *
 *   1. The GUI never accepts what the backend rejects — every `[min, max]` is a
 *      subset of the accepted range. A subset, not an equality: several GUI
 *      caps are deliberate guard rails over a Rust bound that does not exist
 *      (Top K, Max Tokens, Max Tool Iterations), and a narrower field can only
 *      reject input that would have failed validation anyway.
 *   2. A stated default is the real one — and a value Rust deliberately leaves
 *      unset is not given an invented default.
 *
 * The parsing is deliberately narrow: a handful of named functions with a
 * stable literal shape. Every extractor throws with the symbol it could not
 * find rather than returning a default, so restructuring the Rust turns this
 * red instead of silently retiring the guarantee.
 */

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, it, expect } from 'vitest';

import * as settingsDefaults from '../../../src/constants/settingsDefaults';
import { INFERENCE_PARAMS } from '../../../src/constants/inferenceDefaults';
import type { SamplingParamKey } from '../../../src/types';

// vitest runs with the project root as cwd, and the crates live beside it.
function rust(relativePath: string): string {
  return readFileSync(resolve(process.cwd(), relativePath), 'utf8');
}

const SETTINGS_RS = rust('crates/gglib-core/src/settings.rs');
const INFERENCE_RS = rust('crates/gglib-core/src/domain/inference.rs');
const AGENT_CONFIG_RS = rust('crates/gglib-core/src/domain/agent/config.rs');

/** An accepted range. Rust often bounds only one end; the other is infinite. */
interface Range {
  min: number;
  max: number;
}

const num = (literal: string) => Number(literal.replace(/_/g, ''));

/** Every `pub const NAME: T = <number>;` across the files we care about. */
const CONSTANTS: Map<string, number> = new Map(
  [SETTINGS_RS, INFERENCE_RS, AGENT_CONFIG_RS].flatMap((source) =>
    [...source.matchAll(/pub const (\w+):\s*\w+\s*=\s*([0-9_.]+)\s*;/g)].map(
      (match) => [match[1], num(match[2])] as [string, number],
    ),
  ),
);

/**
 * The body of a `-> Self` constructor, as a field → expression map.
 *
 * Sliced to the function's own closing brace rather than a fixed window, so a
 * field added to the struct is picked up instead of silently falling outside.
 */
function structLiteral(source: string, fnName: string): Map<string, string> {
  const start = source.indexOf(`fn ${fnName}(`);
  if (start === -1) throw new Error(`Rust fn ${fnName} not found — did it move or get renamed?`);

  const end = source.indexOf('\n    }', start);
  const body = source.slice(start, end === -1 ? undefined : end);

  const fields = new Map<string, string>();
  for (const match of body.matchAll(/^\s{12}(\w+):\s*(Some\((.*?)\)|None),$/gm)) {
    fields.set(match[1], match[2] === 'None' ? 'None' : match[3]);
  }
  if (fields.size === 0) throw new Error(`No struct fields parsed out of ${fnName}`);
  return fields;
}

/** Resolve a Rust field expression to a number, or null for a deliberate `None`. */
function value(fnName: string, fields: Map<string, string>, field: string): number | null {
  const expression = fields.get(field);
  if (expression === undefined) throw new Error(`${fnName} does not set ${field}`);
  if (expression === 'None') return null;

  // e.g. `crate::domain::agent::DEFAULT_MAX_ITERATIONS as u32` -> the constant.
  const bare = expression.replace(/\s+as\s+\w+$/, '').replace(/^.*::/, '');
  if (/^[0-9_.]+$/.test(bare)) return num(bare);

  const resolved = CONSTANTS.get(bare);
  if (resolved === undefined) throw new Error(`Cannot resolve ${expression} for ${field}`);
  return resolved;
}

/**
 * A top-level function's body, so a search cannot wander into its neighbours
 * or into the test module at the bottom of the file.
 */
function fnBody(source: string, fnName: string): string {
  const start = source.indexOf(`fn ${fnName}(`);
  if (start === -1) throw new Error(`Rust fn ${fnName} not found — did it move or get renamed?`);

  const end = source.indexOf('\n}', start);
  return source.slice(start, end === -1 ? undefined : end);
}

/**
 * The range one `validate_*` guard accepts for one field.
 *
 * Anchored on the guard's subject (`config.top_p` / `settings.proxy_port`) and
 * bounded to that single `if` block. Scanning forward from a bare field name
 * instead would read the *next* parameter's guard — which is exactly how an
 * early draft of this test declared the Repeat Penalty bug clean, by picking up
 * Presence Penalty's inclusive range out of the block below it.
 */
function acceptedRange(fnSource: string, field: string): Range {
  const subject = fnSource.search(new RegExp(`(?:config|settings)\\.${field}\\b`));
  if (subject === -1) {
    throw new Error(`No validation guard found for ${field} — has it stopped being validated?`);
  }

  const rest = fnSource.slice(subject);
  const blockEnd = rest.indexOf('\n    }');
  const guard = rest.slice(0, blockEnd === -1 ? undefined : blockEnd);

  const inclusive = guard.match(/!\(([0-9_.]+)\.\.=([0-9_.]+)\)\.contains/);
  if (inclusive) return { min: num(inclusive[1]), max: num(inclusive[2]) };

  const lessThan = guard.match(/&&\s*\w+\s*<\s*([0-9_.]+)/);
  if (lessThan) return { min: num(lessThan[1]), max: Infinity };

  // `x <= 0.0` rejects zero and below, so anything offered must be strictly
  // greater than zero — a slider starting at 0 is out of bounds.
  if (/&&\s*\w+\s*<=\s*0(\.0)?\b/.test(guard)) return { min: Number.MIN_VALUE, max: Infinity };

  // `x == 0` on an unsigned field means "any positive integer".
  if (/&&\s*\w+\s*==\s*0\b/.test(guard)) return { min: 1, max: Infinity };

  throw new Error(`Unrecognised validation guard for ${field}: ${guard.trim()}`);
}

const VALIDATE_SETTINGS = fnBody(SETTINGS_RS, 'validate_settings');
const VALIDATE_INFERENCE = fnBody(SETTINGS_RS, 'validate_inference_config');

// ── The settings modal's numeric fields ─────────────────────────────────────

const SETTINGS_FIELDS: {
  label: string;
  spec: settingsDefaults.NumericSettingSpec;
  rustField: string;
  /** Absent where Rust bounds the field but `with_defaults()` sets no constant. */
  defaultField?: string;
}[] = [
  {
    label: 'Proxy Server Port',
    spec: settingsDefaults.PROXY_PORT,
    rustField: 'proxy_port',
    defaultField: 'proxy_port',
  },
  {
    label: 'Base Server Port',
    spec: settingsDefaults.LLAMA_BASE_PORT,
    rustField: 'llama_base_port',
    defaultField: 'llama_base_port',
  },
  {
    label: 'Max Download Queue Size',
    spec: settingsDefaults.MAX_DOWNLOAD_QUEUE_SIZE,
    rustField: 'max_download_queue_size',
    defaultField: 'max_download_queue_size',
  },
  {
    label: 'Default Context Size',
    spec: settingsDefaults.CONTEXT_SIZE,
    rustField: 'default_context_size',
    defaultField: 'default_context_size',
  },
];

describe('settings fields vs validate_settings', () => {
  it.each(SETTINGS_FIELDS)('$label offers only values the backend accepts', ({ spec, rustField }) => {
    const accepted = acceptedRange(VALIDATE_SETTINGS, rustField);

    expect(Number(spec.min)).toBeGreaterThanOrEqual(accepted.min);
    expect(Number(spec.max)).toBeLessThanOrEqual(accepted.max);
  });

  it.each(SETTINGS_FIELDS.filter((f) => f.defaultField))(
    '$label states the default Settings::with_defaults() actually uses',
    ({ spec, defaultField }) => {
      const fields = structLiteral(SETTINGS_RS, 'with_defaults');

      expect(Number(spec.default)).toBe(value('with_defaults', fields, defaultField!));
    },
  );

  it('Max Tool Iterations tracks the agent default, and caps only in the UI', () => {
    // No entry in validate_settings: the 1-50 range is a UI guard rail, which
    // is why this one is asserted apart from the table above.
    expect(SETTINGS_RS).not.toMatch(/max_tool_iterations[\s\S]{0,160}?contains/);

    const fields = structLiteral(SETTINGS_RS, 'with_defaults');
    expect(Number(settingsDefaults.MAX_TOOL_ITERATIONS.default)).toBe(
      value('with_defaults', fields, 'max_tool_iterations'),
    );
  });
});

// ── The sampling parameters ─────────────────────────────────────────────────

const RUST_PARAM: Record<SamplingParamKey, string> = {
  temperature: 'temperature',
  topP: 'top_p',
  topK: 'top_k',
  maxTokens: 'max_tokens',
  repeatPenalty: 'repeat_penalty',
  presencePenalty: 'presence_penalty',
  minP: 'min_p',
};

const PARAMS = Object.keys(RUST_PARAM) as SamplingParamKey[];

describe('sampling parameters vs validate_inference_config', () => {
  it.each(PARAMS)('%s offers only values the backend accepts', (param) => {
    const accepted = acceptedRange(VALIDATE_INFERENCE, RUST_PARAM[param]);
    const { min, max } = INFERENCE_PARAMS[param];

    expect(min).toBeGreaterThanOrEqual(accepted.min);
    expect(max).toBeLessThanOrEqual(accepted.max);
  });

  it.each(PARAMS)('%s states the floor with_hardcoded_defaults() actually uses', (param) => {
    const fields = structLiteral(INFERENCE_RS, 'with_hardcoded_defaults');

    // null on both sides is the point for max_tokens: Rust leaves it unset
    // deliberately, so the GUI must not invent one.
    expect(INFERENCE_PARAMS[param].default).toBe(
      value('with_hardcoded_defaults', fields, RUST_PARAM[param]),
    );
  });

  it('only presence_penalty differs in the reasoning floor', () => {
    // inferenceDefaults.ts annotates presencePenalty as the one model-dependent
    // floor. If reasoning_floor() ever overrides a second field, that comment —
    // and the settings-surface caption built on it — goes stale.
    const overrides = [...structLiteral(INFERENCE_RS, 'reasoning_floor').keys()];

    expect(overrides).toEqual(['presence_penalty']);
  });
});
