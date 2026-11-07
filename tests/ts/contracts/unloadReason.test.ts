/**
 * The Load palette's unload sentence is the one the CLI prints.
 * `unload_sentence` in `gglib-cli`'s launch display writes it, and its tests
 * record it for every `UnloadReason` (`UnloadReason::ALL`), beside the
 * reason and the model it names, in `contracts/models/unload_reason.json`;
 * the page's builder must make the same sentence of the same unload, and
 * know exactly the reasons recorded: a reason added in Rust and not here, or
 * one here that Rust no longer has, fails.
 *
 * Nothing here reads Rust source.
 */

import { describe, it, expect } from 'vitest';

import { UNLOAD_BECAUSE, unloadSentence } from '../../../src/readings/launchFit';
import type { Unload } from '../../../src/types/generated/Unload';

import { rust } from './rustSource';

const RECORDED = JSON.parse(rust('contracts/models/unload_reason.json')) as { unload: Unload; sentence: string }[];

describe('the unload sentence is the one the CLI records', () => {
  it('knows exactly the reasons the CLI records', () => {
    expect(RECORDED.length).toBeGreaterThan(1);
    expect(Object.keys(UNLOAD_BECAUSE).sort()).toEqual(RECORDED.map((r) => r.unload.reason).sort());
  });

  it.each(RECORDED.map((r) => [r.unload.reason, r] as const))('%s', (_reason, recorded) => {
    expect(unloadSentence(recorded.unload)).toBe(recorded.sentence);
  });
});
