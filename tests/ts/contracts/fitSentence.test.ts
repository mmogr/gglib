/**
 * The fit sentence and the Context note are Rust's. `ContextFit::new` in
 * gglib-core composes them from the fit's figures and the daemon serves them
 * on `plan.fit`; gglib-core's fit-sentence tests record a fit of each shape,
 * words and all, in `contracts/models/fit_sentence.json`.
 *
 * Here: the recording is the wire shape the GUI is generated against (every
 * field of `ContextFit`, no other), the fixture the GUI's tests use is the
 * recording's first fit, words included, and the Load palette's line says
 * each recorded fit's sentence as served. The inspector's reading of the
 * same fields is pinned in `places/inspector.test.tsx`.
 *
 * Nothing here composes a sentence.
 */

import { describe, it, expect } from 'vitest';

import { fitLine } from '../../../src/places/endpoint/paletteRows';
import type { ContextFit } from '../../../src/types/generated/ContextFit';
import { chatPlan, launchExplanation, samplingExplanation } from '../fixtures/explain';

import { rust } from './rustSource';

const RECORDED = JSON.parse(rust('contracts/models/fit_sentence.json')) as ContextFit[];

/** Every field of the generated type: the compiler refuses a missing or extra key. */
const FIELDS: Record<keyof ContextFit, true> = {
  servesAt: true,
  source: true,
  fitted: true,
  budgetBytes: true,
  weightsBytes: true,
  kvBytesPerToken: true,
  kvBytes: true,
  trainedCtx: true,
  unsnapped: true,
  sentence: true,
  note: true,
};

describe('the fit sentence is the one Rust records', () => {
  it('records a fit of each shape, each with exactly the wire fields', () => {
    expect(RECORDED.length).toBeGreaterThan(5);
    for (const fit of RECORDED) expect(Object.keys(fit).sort()).toEqual(Object.keys(FIELDS).sort());
    // A fit, one above the fit, a refusal: the three sentence shapes.
    expect(RECORDED.some((f) => f.sentence.startsWith('Fits this machine'))).toBe(true);
    expect(RECORDED.some((f) => f.fitted !== null && f.sentence.endsWith('fits this machine.'))).toBe(true);
    expect(RECORDED.some((f) => f.sentence.includes('could not fit one'))).toBe(true);
  });

  it("is the fixture's fit, words and all", () => {
    expect(chatPlan().fit).toEqual(RECORDED[0]);
  });

  it.each(RECORDED.map((f) => [f.sentence, f] as const))('the Load palette says "%s"', (sentence, fit) => {
    expect(fitLine(launchExplanation(samplingExplanation(), chatPlan({ fit })))).toBe(sentence);
  });
});
