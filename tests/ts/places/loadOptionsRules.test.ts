/**
 * What stops a load from the Load with options sheet (`problem`), one row
 * per sentence and the rows that pass; and what the launch and the per-model
 * patch carry of what was typed (`launchConfig`, `defaultsPatch`), pure.
 */

import { describe, it, expect } from 'vitest';

import { UNTOUCHED, defaultsPatch, launchConfig, problem, type Typed } from '../../../src/inspector/loadOptions';
import { guiModel } from '../fixtures/model';

const typed = (change: Partial<Typed>): Typed => ({ ...UNTOUCHED, ...change });

describe('what stops a load', () => {
  it.each<[string, Partial<Typed>, boolean, string | null]>([
    ['nothing typed', {}, false, null],
    ['a context', { context: '16384' }, false, null],
    ['a context of 0', { context: '0' }, false, 'Context is a whole number above 0.'],
    ['a fractional context', { context: '1.5' }, false, 'Context is a whole number above 0.'],
    ['a port in range', { port: '9300' }, false, null],
    ['a privileged port', { port: '80' }, false, 'Port is a whole number from 1024 to 65535.'],
    ['a port past the range', { port: '65536' }, false, 'Port is a whole number from 1024 to 65535.'],
    ['a draft count of 0', { nMax: '0' }, false, 'Draft tokens is a whole number above 0.'],
    ['a min probability past 1', { pMin: '1.5' }, false, 'Min probability is from 0 to 1.'],
    ['a min probability below 0', { pMin: '-0.1' }, false, 'Min probability is from 0 to 1.'],
    ['a min probability of 1', { pMin: '1' }, false, null],
    ['MTP on for this load only, nothing drafting, no count', { mtp: true, thisLoadOnly: true }, false, 'Say how many draft tokens for this load.'],
    ['the same with a count', { mtp: true, thisLoadOnly: true, nMax: '3' }, false, null],
    ['the same where the plan drafts already', { mtp: true, thisLoadOnly: true }, true, null],
    ['MTP on saved as the model’s own, no count', { mtp: true }, false, null],
  ])('%s', (_name, change, mtpOn, said) => {
    expect(problem(typed(change), mtpOn)).toBe(said);
  });
});

describe('what the launch and the patch carry', () => {
  it('sends 0 for MTP off whatever count was typed before it', () => {
    expect(launchConfig({ id: 1 }, typed({ mtp: false, nMax: '4', pMin: '0.6' }))).toEqual({ id: 1, mlock: false, mtpDraftNMax: 0 });
  });

  it('patches nothing for this load only, and only the typed fields otherwise', () => {
    const model = guiModel({ id: 1, serverDefaults: { contextLength: 8192, jinja: true } });
    expect(defaultsPatch(model, typed({ context: '16384', port: '9300', thisLoadOnly: true }))).toBeNull();
    expect(defaultsPatch(model, typed({ port: '9300', pin: true, nMax: '3' }))).toBeNull();
    expect(defaultsPatch(model, typed({ jinja: false }))).toEqual({ serverDefaults: { contextLength: 8192, jinja: false } });
  });
});
