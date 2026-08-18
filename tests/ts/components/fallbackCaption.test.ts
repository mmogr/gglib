/**
 * What the inference form says under an empty field.
 *
 * The module's rule is that it formats what `GET /api/models/:id/explain`
 * returns and never works anything out for itself. These pin the place that
 * rule is easiest to break: the auto-detected rung, where three different
 * origins share one layer name and only `defaultsOrigin` tells them apart.
 *
 * Three, not four: `user` is the one origin that cannot reach this rung.
 * Resolution picks `modelUserSet` for it and `modelAutoDetected` for the other
 * three, so a fixture pairing `user` with this layer would describe a state
 * the backend does not produce, and would pin wording nobody ever sees.
 */

import { describe, it, expect } from 'vitest';

import {
  fallbackCaption,
  type InferenceFallback,
} from '../../../src/components/InferenceParametersForm/fallbackCaption';
import type { DefaultsOriginName, SamplingExplanation } from '../../../src/types';
import { resolvedConfig } from '../fixtures/inference';

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
    explanation: {
      resolved: resolvedConfig({ temperature: 1.2 }),
      sources: [{ param: 'temperature', kind: 'layer', layer: 'modelAutoDetected' }],
      profile: null,
      isReasoning: false,
      trustClientSampling: false,
      defaultsOrigin,
    } satisfies SamplingExplanation,
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

  it('names the tag guess when that is what it actually was', () => {
    expect(fallbackCaption('temperature', explaining('auto_detected'))).toContain(
      'per-model defaults (auto-detected: reasoning tag)',
    );
  });

  /**
   * A backend too old to send the field, or a model with no origin stored.
   * The tag guess is the pre-existing wording for that case and stays.
   */
  it('keeps the old wording when the backend sent no origin at all', () => {
    expect(fallbackCaption('temperature', explaining(undefined))).toContain(
      'per-model defaults (auto-detected: reasoning tag)',
    );
  });

  it('still reports the resolved value beside the source', () => {
    expect(fallbackCaption('temperature', explaining('measured'))).toMatch(/^Resolves to 1\.2/);
  });
});
