/**
 * Drift guards for PR8's settings-parity additions, in the style of
 * `settingsBounds.test.ts`: read the Rust source off disk and assert the
 * GUI's transcriptions match, so the two surfaces cannot drift silently.
 */

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { describe, it, expect } from 'vitest';
import { MAX_STAGNATION_STEPS } from '../../../src/constants/settingsDefaults';
import { REASONING_EFFORT_LEVELS } from '../../../src/constants/reasoningEffort';
import { STARTER_PROFILES } from '../../../src/components/SettingsModal/InferenceProfiles';

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

describe('starter profiles mirror builtin_templates()', () => {
  const FILE = rust('crates/gglib-core/src/domain/inference_profile.rs');
  // Anchored and uniqueness-checked, per this directory's rule: an unanchored
  // `indexOf` takes the first match, so a same-named decoy declared earlier
  // satisfies the guard for a function that has drifted.
  //
  // `builtin_templates` is now a composition of two families, so the guard
  // reads each family's own function. The composition itself is checked below,
  // which is what stops a family being dropped from the installed set while
  // its literals stay in the file.
  const SAMPLING = fnSource(FILE, 'sampling_templates');
  const REASONING = fnSource(FILE, 'reasoning_templates');
  const BUILTIN = fnSource(FILE, 'builtin_templates');

  /** Extract one template's literal block from the sampling family by name. */
  const templateBlock = (name: string): string => {
    const index = SAMPLING.indexOf(`name: "${name}"`);
    expect(index, `template "${name}" not found in sampling_templates()`).toBeGreaterThan(-1);
    return SAMPLING.slice(index, SAMPLING.indexOf('list_in_models', index) + 40);
  };

  it('installs both families, so neither can be quietly dropped', () => {
    expect(BUILTIN).toContain('sampling_templates()');
    expect(BUILTIN).toContain('reasoning_templates()');
  });

  it('transcribes every sampling template the Rust defines, and no others', () => {
    const rustNames = [...SAMPLING.matchAll(/name: "([a-z-]+)"\.to_owned\(\)/g)].map((m) => m[1]);
    expect(rustNames.length).toBeGreaterThan(0);
    expect(STARTER_PROFILES.map((p) => p.name).sort()).toEqual([...new Set(rustNames)].sort());
  });

  it.each(STARTER_PROFILES.map((p) => [p.name, p] as const))(
    'matches the Rust literal for %s',
    (_name, profile) => {
      const block = templateBlock(profile.name);
      expect(block).toContain(`temperature: Some(${profile.config.temperature})`);
      expect(block).toContain(`top_p: Some(${profile.config.topP})`);
      expect(block).toContain(`list_in_models: ${profile.listInModels}`);
      if (profile.description) {
        expect(block).toContain(`Some("${profile.description}".to_owned())`);
      }
    },
  );

  it('keeps templates sparse — temperature and top-p only', () => {
    for (const profile of STARTER_PROFILES) {
      expect(Object.keys(profile.config).sort()).toEqual(['temperature', 'topP']);
    }
  });

  /**
   * The reasoning family is still *not* in `STARTER_PROFILES`, and the reason
   * has changed: the GUI's `InferenceConfig` now carries `reasoningEffort` and
   * `reasoningBudgetTokens`, so the blocker is no longer the type.
   *
   * Nor is it `InferenceProfileEditor` any more. That form used to rebuild a
   * profile's config from its own eleven-name `PARAMS` list and drop every
   * key not on it — seven of `InferenceConfig`'s eighteen fields, both
   * reasoning controls included — so seeding a `high` profile here would have
   * installed something the first edit silently emptied. The editor now
   * derives its list from `INFERENCE_CONFIG_KEYS` and renders the effort
   * control, so that blocker is gone.
   *
   * What remains is simply that nobody has transcribed the family. The guard
   * stays: it is what fails when somebody does, so the pairing of effort with
   * budget gets checked against the Rust rather than assumed.
   */
  it('defines a reasoning family the GUI does not transcribe yet', () => {
    const rungs = [...REASONING.matchAll(/\("([a-z]+)", ReasoningEffort::/g)].map((m) => m[1]);
    expect(rungs).toEqual(['minimal', 'low', 'medium', 'high', 'xhigh', 'max']);

    // Each rung sets both halves: an effort alone is inert on a template that
    // does not read the variable, which is the whole reason for the pairing.
    expect(REASONING).toContain('reasoning_effort: Some(effort)');
    expect(REASONING).toContain('reasoning_budget_tokens: Some(budget)');

    const guiNames = STARTER_PROFILES.map((p) => p.name);
    for (const rung of rungs) {
      expect(
        guiNames,
        'teach InferenceProfileEditor the reasoning fields before seeding these',
      ).not.toContain(rung);
    }
  });

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
