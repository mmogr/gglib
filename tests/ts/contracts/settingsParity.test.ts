/**
 * Drift guards for PR8's settings-parity additions: read the Rust source off
 * disk and assert the GUI's transcriptions match, so the two surfaces cannot
 * drift silently.
 */

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, it, expect } from 'vitest';
import { MAX_STAGNATION_STEPS } from '../../../src/constants/settingsDefaults';
import { REASONING_EFFORT_LEVELS } from '../../../src/constants/reasoningEffort';

import { fnSource, rust } from './rustSource';

/**
 * The GUI's effort ladder against `ReasoningEffort`'s own wire spellings.
 *
 * Read from `as_str` rather than from the variant names: llama-server spells
 * the fifth rung `xhigh`, one word, and `XHigh` would transcribe to `x_high`
 * under any rule but the `lowercase` one serde actually applies. The wire is
 * what the GUI puts in a request body, so the wire is what this compares —
 * and upstream never validates the string, so a mis-transcribed level renders
 * into the prompt verbatim instead of being rejected.
 */
describe('the GUI effort ladder mirrors ReasoningEffort', () => {
  const AS_STR = fnSource(rust('crates/gglib-core/src/domain/reasoning_effort.rs'), 'as_str', '\n    }');

  it('lists every level, in the Rust order, spelled the way the wire spells it', () => {
    const levels = [...AS_STR.matchAll(/Self::\w+ => "([a-z]+)"/g)].map((match) => match[1]);

    expect(levels).toHaveLength(6);
    expect([...REASONING_EFFORT_LEVELS]).toEqual(levels);
  });

  it('offers no "none", which erases the kwarg rather than naming a level', () => {
    // ADR 0007 finding 4: llama-server treats "none" specially, and gpt-oss's
    // template then falls back to `medium` — so a "none" option would read as
    // "do not think" and deliver the template's own default.
    expect([...REASONING_EFFORT_LEVELS]).not.toContain('none');
    expect(AS_STR).not.toContain('"none"');
  });
});

/**
 * The starter profiles have no copy on this side: the settings page asks the
 * daemon to install them, by the function `gglib config profile
 * install-templates` runs. What this side guards is the editor, which has to
 * keep every field one of the nine sets.
 */
describe('the profile editor keeps what a starter profile sets', () => {
  it('rebuilds a profile from a list it cannot silently shorten', () => {
    // This used to assert the opposite — that the editor iterated a hand-kept
    // `PARAMS` and therefore dropped everything else. That was true and it
    // was the bug: eleven of `InferenceConfig`'s eighteen fields were listed,
    // so `gglib config profile set --top-n-sigma 3` followed by an edit in
    // the GUI silently emptied the field.
    //
    // The list is now derived from `INFERENCE_CONFIG_KEYS`, and
    // `ProfileParamsAreComplete` fails the build if a new `InferenceConfig`
    // field is neither a `SamplingParamKey` nor one of the two named
    // exclusions. The type is the real guard; this checks the mechanism is
    // still the one in place.
    const editor = readFileSync(
      // Resolved against this file, not the cwd, for the reason `rustSource`
      // states: an IDE runner invoked elsewhere would ENOENT instead of
      // failing with something it can explain.
      resolve(import.meta.dirname, '../../../src/components/SettingsModal/InferenceProfileEditor.tsx'),
      'utf8',
    );

    expect(editor).toContain('INFERENCE_CONFIG_KEYS.filter');
    expect(editor).toContain('ProfileParamsAreComplete');
    expect(editor).toContain('for (const key of PROFILE_PARAM_KEYS)');
    // `seed` stays out, and deliberately: a profile is reused across every
    // request that selects it, so a seed would pin them all to one output.
    // `crates/gglib-cli/.../profiles.rs` hard-codes `seed: None` for the same
    // reason and offers no `--seed` flag.
    expect(editor).toContain('"seed"');
    // The enum cannot ride the numeric loop — `Number()` would make it NaN.
    expect(editor).toContain('config.reasoningEffort = reasoningEffort');
  });
});

describe('max stagnation steps tracks the agent default', () => {
  it('has no validate_settings bound, capping only in the UI', () => {
    const SETTINGS_RS = rust('crates/gglib-core/src/settings.rs');
    expect(SETTINGS_RS).not.toMatch(/max_stagnation_steps[\s\S]{0,160}?contains/);
  });

  it('states the Rust default and ceiling', () => {
    const CONFIG_RS = rust('crates/gglib-core/src/domain/agent/config.rs');
    const defaultMatch = CONFIG_RS.match(/DEFAULT_MAX_STAGNATION_STEPS[^=]*=\s*(\d+)/);
    const ceilingMatch = CONFIG_RS.match(/MAX_STAGNATION_STEPS_CEILING[^=]*=\s*(\d+)/);
    expect(defaultMatch, 'DEFAULT_MAX_STAGNATION_STEPS not found').not.toBeNull();
    expect(ceilingMatch, 'MAX_STAGNATION_STEPS_CEILING not found').not.toBeNull();
    expect(MAX_STAGNATION_STEPS.default).toBe(defaultMatch![1]);
    expect(MAX_STAGNATION_STEPS.max).toBe(ceilingMatch![1]);
  });
});
