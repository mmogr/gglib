/**
 * Drift guards for PR8's settings-parity additions, in the style of
 * `settingsBounds.test.ts`: read the Rust source off disk and assert the
 * GUI's transcriptions match, so the two surfaces cannot drift silently.
 */

import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { MAX_STAGNATION_STEPS } from '../../../src/constants/settingsDefaults';
import { STARTER_PROFILES } from '../../../src/components/SettingsModal/InferenceProfiles';

// Relative to this file, not the cwd: `vitest --root` moves the cwd.
const REPO_ROOT = resolve(import.meta.dirname, '../../..');
const rust = (path: string) => readFileSync(resolve(REPO_ROOT, path), 'utf8');

describe('starter profiles mirror builtin_templates()', () => {
  const FILE = rust('crates/gglib-core/src/domain/inference_profile.rs');
  const start = FILE.indexOf('pub fn builtin_templates()');
  const SOURCE = FILE.slice(start, FILE.indexOf('\n}', start));

  /** Extract one template's literal block from the function body by name. */
  const templateBlock = (name: string): string => {
    const index = SOURCE.indexOf(`name: "${name}"`);
    expect(index, `template "${name}" not found in builtin_templates()`).toBeGreaterThan(-1);
    return SOURCE.slice(index, SOURCE.indexOf('list_in_models', index) + 40);
  };

  it('transcribes every template the Rust defines, and no others', () => {
    const rustNames = [...SOURCE.matchAll(/name: "([a-z-]+)"\.to_owned\(\)/g)].map((m) => m[1]);
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
