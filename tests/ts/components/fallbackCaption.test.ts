/**
 * What the inference form says under an empty field.
 *
 * The module's rule is that it formats what `GET /api/models/:id/explain`
 * returns and never works anything out for itself. Where a value came from
 * is the daemon's label on its source (`source_label` in gglib-core, which
 * names the recipe rung by its origin); the caption shows it as served.
 */

import { describe, it, expect } from 'vitest';

import {
  fallbackCaption,
  type InferenceFallback,
} from '../../../src/components/SamplingForm/fallbackCaption';
import { resolvedConfig } from '../fixtures/inference';
import { samplingExplanation } from '../fixtures/explain';

/**
 * An explanation attributing temperature to the auto-detected rung, labelled
 * as the daemon labels a published recipe.
 *
 * `ownLayer` is `modelUserSet` — the rung `ModelEditForm` edits, and so the
 * surface this caption actually appears on. It differs from the entry's own
 * layer, which is what keeps `usableEntry` from reading the entry as one the
 * form is in the middle of clearing.
 */
const explaining: InferenceFallback = {
  kind: 'resolved',
  ownLayer: 'modelUserSet',
  resolution: {
    isLoading: false,
    hasError: false,
    explanation: samplingExplanation({
      resolved: resolvedConfig({ temperature: 1.2 }),
      sources: [
        {
          param: 'temperature',
          kind: 'layer',
          layer: 'modelAutoDetected',
          label: 'per-model defaults (published by the model author)',
        },
      ],
      defaultsOrigin: 'published',
    }),
  },
};

describe('fallbackCaption for a resolved field', () => {
  it('says the value and the source as the daemon labels it', () => {
    expect(fallbackCaption('temperature', explaining)).toBe(
      'Resolves to 1.2 — per-model defaults (published by the model author)',
    );
  });
});

/** An explanation that resolves nothing, with `field` unset by design. */
const unset = (field: 'temperature' | 'maxTokens'): InferenceFallback => ({
  kind: 'resolved',
  ownLayer: 'modelUserSet',
  resolution: {
    isLoading: false,
    hasError: false,
    explanation: samplingExplanation({ sources: [{ param: field, kind: 'unset', label: 'unset by design' }] }),
  },
});

describe('fallbackCaption for a field nothing resolves', () => {
  it('never gives a sampler the token limit', () => {
    const caption = fallbackCaption('temperature', unset('temperature'));
    expect(caption).toBe('Unset — unset by design');
    expect(caption).not.toMatch(/limit|16,384/);
  });

  it('gives Max Tokens the chat turn limit', () => {
    expect(fallbackCaption('maxTokens', unset('maxTokens'))).toContain(
      'a chat turn gets at least 16,384 tokens',
    );
  });
});
