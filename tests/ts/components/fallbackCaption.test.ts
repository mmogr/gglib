/**
 * What the inference form says under an empty field.
 *
 * The module's rule is that it formats what `GET /api/models/:id/explain`
 * returns and never works anything out for itself. These pin the place that
 * rule is easiest to break: the recipe rung, where three different recipes
 * share one layer name and only `defaultsOrigin` tells them apart — a
 * published one, a measured one, and the `reasoning` recipe, which is never
 * stored and so arrives with no origin, or beside a person's own `user`
 * defaults on the rung above.
 */

import { describe, it, expect } from 'vitest';

import {
  fallbackCaption,
  type InferenceFallback,
} from '../../../src/components/InferenceParametersForm/fallbackCaption';
import type { DefaultsOriginName } from '../../../src/types';
import { resolvedConfig } from '../fixtures/inference';
import { samplingExplanation } from '../fixtures/explain';

/**
 * An explanation attributing temperature to the auto-detected rung.
 *
 * `ownLayer` is `modelUserSet` — the rung `ModelEditForm` edits, and so the
 * surface this caption actually appears on. It differs from the entry's own
 * layer, which is what keeps `usableEntry` from reading the entry as one the
 * form is in the middle of clearing.
 */
const explaining = (defaultsOrigin?: DefaultsOriginName | null): InferenceFallback => ({
  kind: 'resolved',
  ownLayer: 'modelUserSet',
  resolution: {
    isLoading: false,
    hasError: false,
    explanation: samplingExplanation({
      resolved: resolvedConfig({ temperature: 1.2 }),
      sources: [{ param: 'temperature', kind: 'layer', layer: 'modelAutoDetected' }],
      defaultsOrigin: defaultsOrigin ?? null,
    }),
  },
});

describe('fallbackCaption on the auto-detected rung', () => {
  it("names the model author when the origin is the author's own recipe", () => {
    expect(fallbackCaption('temperature', explaining('published'))).toContain(
      'per-model defaults (published by the model author)',
    );
  });

  it('names the sweep when the origin was measured', () => {
    expect(fallbackCaption('temperature', explaining('measured'))).toContain(
      'per-model defaults (measured by a tune sweep)',
    );
  });

  /**
   * The `reasoning` recipe is never stored, so it arrives with no origin, or
   * with `user` when a person set some fields of their own on the rung above.
   */
  it('names the tag recipe when nothing published or measured is stored', () => {
    for (const origin of [undefined, null, 'user'] as const) {
      expect(fallbackCaption('temperature', explaining(origin))).toContain(
        'per-model defaults (auto-detected: reasoning tag)',
      );
    }
  });

  it('still reports the resolved value beside the source', () => {
    expect(fallbackCaption('temperature', explaining('measured'))).toMatch(/^Resolves to 1\.2/);
  });
});
