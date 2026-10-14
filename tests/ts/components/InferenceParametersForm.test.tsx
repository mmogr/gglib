import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

import { InferenceParametersForm } from '../../../src/components/InferenceParametersForm';
import type { InferenceFallback } from '../../../src/components/InferenceParametersForm/fallbackCaption';
import type { SparseInferenceConfig, SamplingExplanation } from '../../../src/types';
import { resolvedConfig } from '../fixtures/inference';
import { samplingExplanation } from '../fixtures/explain';

/**
 * Every parameter, with the values a user should see on the settings surface.
 *
 * Literals rather than imports from inferenceDefaults.ts: a component test that
 * reads the same constant it asserts on would pass whatever that constant said.
 * settingsBounds.test.ts is what ties these to Rust.
 */
const PARAMS: { label: string; min: number; max: number }[] = [
  { label: 'Temperature', min: 0, max: 2 },
  { label: 'Top P', min: 0, max: 1 },
  { label: 'Top K', min: 1, max: 200 },
  { label: 'Max Tokens', min: 1, max: 32768 },
  { label: 'Repeat Penalty', min: 0.05, max: 2 },
  { label: 'Presence Penalty', min: 0, max: 2 },
  { label: 'Min P', min: 0, max: 1 },
  { label: 'DRY Multiplier', min: 0, max: 2 },
  { label: 'DRY Base', min: 1.05, max: 4 },
  { label: 'DRY Allowed Length', min: 0, max: 20 },
  { label: 'DRY Penalty Last N', min: -1, max: 8192 },
];

const UPSTREAM: InferenceFallback = { kind: 'upstream' };

/** An explanation in which every parameter came from the global settings. */
function fromGlobal(overrides: Partial<SamplingExplanation> = {}): SamplingExplanation {
  return samplingExplanation({
    resolved: resolvedConfig({
      temperature: 1.2,
      topP: 0.8,
      topK: 60,
      maxTokens: 512,
      repeatPenalty: 1.15,
      presencePenalty: 0.5,
      minP: 0.05,
    }),
    sources: [
      { param: 'temperature', kind: 'layer', layer: 'global' },
      { param: 'topP', kind: 'layer', layer: 'global' },
      { param: 'topK', kind: 'layer', layer: 'global' },
      { param: 'presencePenalty', kind: 'layer', layer: 'global' },
      { param: 'repeatPenalty', kind: 'layer', layer: 'global' },
      { param: 'minP', kind: 'layer', layer: 'global' },
      { param: 'maxTokens', kind: 'layer', layer: 'global' },
    ],
    ...overrides,
  });
}

function resolved(
  explanation: SamplingExplanation | null,
  state: { isLoading?: boolean; hasError?: boolean } = {},
): InferenceFallback {
  return {
    kind: 'resolved',
    ownLayer: 'modelUserSet',
    resolution: { explanation, isLoading: false, hasError: false, ...state },
  };
}

function renderForm(fallback: InferenceFallback, value?: SparseInferenceConfig) {
  const onChange = vi.fn();
  render(<InferenceParametersForm value={value} onChange={onChange} fallback={fallback} />);
  return onChange;
}

describe('InferenceParametersForm', () => {
  it.each(PARAMS)('$label is reachable by its label', ({ label }) => {
    renderForm(UPSTREAM);

    expect(screen.getByLabelText(label)).toBeInTheDocument();
  });

  it.each(PARAMS)('$label constrains input to the range the backend accepts', ({ label, min, max }) => {
    renderForm(UPSTREAM);

    const control = screen.getByLabelText(label);
    expect(control).toHaveAttribute('min', String(min));
    expect(control).toHaveAttribute('max', String(max));
  });

  it('will not let Repeat Penalty reach zero, which the backend rejects', () => {
    renderForm(UPSTREAM);

    // validate_inference_config rejects repeat_penalty <= 0.0, so the left
    // stop of the track used to be a value that could not be saved.
    expect(Number(screen.getByLabelText('Repeat Penalty').getAttribute('min'))).toBeGreaterThan(0);
  });

  describe('on the global settings surface, where llama.cpp\'s defaults apply', () => {
    // There is no floor (ADR 0022), so no field states a default of gglib's
    // own and none names a value for reasoning models.
    it.each(PARAMS)('$label states no default of gglib\'s own', ({ label }) => {
      renderForm(UPSTREAM);

      const field = screen.getByLabelText(label);
      expect(field).not.toHaveAccessibleDescription(/^Default: /);
      expect(field).not.toHaveAccessibleDescription(/for reasoning models/);
    });

    // Max Tokens' "No limit" wording describes unbounded generation. Every
    // other parameter means something different: gglib names no value and
    // llama.cpp's own default applies — a number worth naming, and nothing to
    // do with a limit. A blank caption would read as "nothing happens here",
    // which is the opposite of true.
    it.each([
      ['Temperature', /llama\.cpp default: 0\.8/],
      ['Top P', /llama\.cpp default: 0\.95/],
      ['Top K', /llama\.cpp default: 40/],
      ['Repeat Penalty', /llama\.cpp default: 1\.0/],
      ['Presence Penalty', /llama\.cpp default: 0\.0/],
      ['Min P', /llama\.cpp default: 0\.05/],
      ['DRY Multiplier', /llama\.cpp default: 0\.0 \(DRY off\)/],
      ['DRY Base', /llama\.cpp default: 1\.75/],
      ['DRY Allowed Length', /llama\.cpp default: 2/],
      ['DRY Penalty Last N', /llama\.cpp default: 64/],
    ])('%s cites llama.cpp rather than borrowing Max Tokens wording', (label, pattern) => {
      renderForm(UPSTREAM);

      const field = screen.getByLabelText(label);
      expect(field).toHaveAccessibleDescription(pattern);
      expect(field).not.toHaveAccessibleDescription(/No limit/);
    });

    it('claims no default for Max Tokens, because the backend has none', () => {
      renderForm(UPSTREAM);

      const maxTokens = screen.getByLabelText('Max Tokens');
      expect(maxTokens).not.toHaveAttribute('placeholder');
      expect(maxTokens).toHaveAccessibleDescription(/No limit/);
      expect(maxTokens).not.toHaveAccessibleDescription(/2,?048/);
    });
  });

  describe('on a model surface, where a lower layer applies', () => {
    // Values are formatted by the same helper the inspector's Sampling
    // section uses, so the two surfaces read identically.
    it.each([
      { label: 'Temperature', value: '1.2' },
      { label: 'Top K', value: '60' },
    ])('$label reports what it resolves to and where from', ({ label, value }) => {
      renderForm(resolved(fromGlobal()));

      expect(screen.getByLabelText(label)).toHaveAccessibleDescription(
        new RegExp(`Resolves to ${value} — global settings`),
      );
    });

    it('never states a fixed default as though it were the answer', () => {
      renderForm(resolved(fromGlobal()));

      // The bug this replaced: a slider parked at 0.70 reading "(default)"
      // while the global setting said 1.2.
      const temperature = screen.getByLabelText('Temperature');
      expect(temperature).not.toHaveAccessibleDescription(/0\.7/);
      expect(temperature).toHaveValue('1.2');
    });

    it('puts the placeholder on the inherited value', () => {
      renderForm(resolved(fromGlobal()));

      expect(screen.getByLabelText('Top K')).toHaveAttribute('placeholder', '60');
    });

    it('says nothing about a value supplied by the layer this form edits', () => {
      // The explanation describes what is saved. If the field is empty here
      // but the model layer still owns the value, the user has just cleared
      // it and the saved provenance is one save out of date.
      renderForm(
        resolved(
          fromGlobal({
            sources: [{ param: 'temperature', kind: 'layer', layer: 'modelUserSet' }],
          }),
        ),
      );

      expect(screen.getByLabelText('Temperature')).not.toHaveAccessibleDescription();
    });

    it('claims nothing while the explanation is still loading', () => {
      renderForm(resolved(null, { isLoading: true }));

      expect(screen.getByLabelText('Temperature')).not.toHaveAccessibleDescription();
    });

    it('says so when the resolution cannot be fetched', () => {
      renderForm(resolved(null, { hasError: true }));

      // Falling back to a hardcoded number here would be the original bug.
      const temperature = screen.getByLabelText('Temperature');
      expect(temperature).toHaveAccessibleDescription(/Resolution unavailable/);
      expect(temperature).not.toHaveAccessibleDescription(/0\.7/);
    });
  });

  describe('editing', () => {
    it('reports a typed value to onChange', async () => {
      const onChange = renderForm(UPSTREAM);

      await userEvent.type(screen.getByLabelText('Top K'), '5');

      expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ topK: 5 }));
    });

    it('drops the caption once a value is set, and offers a reset', () => {
      renderForm(UPSTREAM, { topK: 80 });

      expect(screen.getByLabelText('Top K')).toHaveValue(80);
      expect(screen.getByLabelText('Top K')).not.toHaveAccessibleDescription();
      expect(screen.getByRole('button', { name: /Reset Top K to default/ })).toBeInTheDocument();
    });

    it('clears the parameter when reset is pressed', async () => {
      const onChange = renderForm(UPSTREAM, { topK: 80 });

      await userEvent.click(screen.getByRole('button', { name: /Reset Top K to default/ }));

      expect(onChange).toHaveBeenCalledWith({});
    });

    // The form holds a *sparse* config, and clearing a field deletes its key.
    // Typing this state as the wire's `InferenceConfig` — where all eighteen
    // keys are present — would make `delete` a type error.
    //
    // That is the whole of it, and the reason `SparseInferenceConfig` exists.
    // Nothing downstream tells a present `null` from an absent key: the reset
    // button tests `!== undefined && !== null`, `hasInferenceOverrides` tests
    // `!= null`, and the serve mapper tests `!= null` — deliberately, because
    // Rust cannot tell them apart either. So this pins the one behaviour that
    // would change: clearing must reach `onChange` as an object without the
    // key, not as one carrying `null`.
    it('un-sets reasoning effort by dropping the key, not by nulling it', async () => {
      const onChange = renderForm(UPSTREAM, { reasoningEffort: 'high' });

      await userEvent.selectOptions(screen.getByLabelText('Reasoning Effort'), '');

      expect(onChange).toHaveBeenCalledWith({});
      const sent = onChange.mock.calls.at(-1)?.[0];
      expect(Object.hasOwn(sent, 'reasoningEffort')).toBe(false);
    });
  });
});
