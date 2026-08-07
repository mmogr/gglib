import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

import { InferenceParametersForm } from '../../../src/components/InferenceParametersForm';
import type { InferenceFallback } from '../../../src/components/InferenceParametersForm/fallbackCaption';
import type { InferenceConfig, SamplingExplanation } from '../../../src/types';

/**
 * Every parameter, with the values a user should see on the settings surface.
 *
 * Literals rather than imports from inferenceDefaults.ts: a component test that
 * reads the same constant it asserts on would pass whatever that constant said.
 * settingsBounds.test.ts is what ties these to Rust.
 */
const PARAMS: { label: string; floor: string | null; min: number; max: number }[] = [
  { label: 'Temperature', floor: '0.7', min: 0, max: 2 },
  { label: 'Top P', floor: '0.95', min: 0, max: 1 },
  { label: 'Top K', floor: '40', min: 1, max: 200 },
  { label: 'Max Tokens', floor: null, min: 1, max: 8192 },
  { label: 'Repeat Penalty', floor: '1.0', min: 0.05, max: 2 },
  { label: 'Presence Penalty', floor: '0.0', min: 0, max: 2 },
  { label: 'Min P', floor: '0.0', min: 0, max: 1 },
];

const FLOOR: InferenceFallback = { kind: 'floor' };

/** An explanation in which every parameter came from the global settings. */
function fromGlobal(overrides: Partial<SamplingExplanation> = {}): SamplingExplanation {
  return {
    resolved: {
      temperature: 1.2,
      topP: 0.8,
      topK: 60,
      maxTokens: 512,
      repeatPenalty: 1.15,
      presencePenalty: 0.5,
      minP: 0.05,
    },
    sources: [
      { param: 'temperature', kind: 'layer', layer: 'global' },
      { param: 'topP', kind: 'layer', layer: 'global' },
      { param: 'topK', kind: 'layer', layer: 'global' },
      { param: 'presencePenalty', kind: 'layer', layer: 'global' },
      { param: 'repeatPenalty', kind: 'layer', layer: 'global' },
      { param: 'minP', kind: 'layer', layer: 'global' },
      { param: 'maxTokens', kind: 'layer', layer: 'global' },
    ],
    profile: null,
    isReasoning: false,
    trustClientSampling: false,
    ...overrides,
  };
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

function renderForm(fallback: InferenceFallback, value?: InferenceConfig) {
  const onChange = vi.fn();
  render(<InferenceParametersForm value={value} onChange={onChange} fallback={fallback} />);
  return onChange;
}

describe('InferenceParametersForm', () => {
  it.each(PARAMS)('$label is reachable by its label', ({ label }) => {
    renderForm(FLOOR);

    expect(screen.getByLabelText(label)).toBeInTheDocument();
  });

  it.each(PARAMS)('$label constrains input to the range the backend accepts', ({ label, min, max }) => {
    renderForm(FLOOR);

    const control = screen.getByLabelText(label);
    expect(control).toHaveAttribute('min', String(min));
    expect(control).toHaveAttribute('max', String(max));
  });

  it('will not let Repeat Penalty reach zero, which the backend rejects', () => {
    renderForm(FLOOR);

    // validate_inference_config rejects repeat_penalty <= 0.0, so the left
    // stop of the track used to be a value that could not be saved.
    expect(Number(screen.getByLabelText('Repeat Penalty').getAttribute('min'))).toBeGreaterThan(0);
  });

  describe('on the global settings surface, where the floor is what applies', () => {
    it.each(PARAMS.filter((p) => p.floor !== null))('$label states the floor', ({ label, floor }) => {
      renderForm(FLOOR);

      expect(screen.getByLabelText(label)).toHaveAccessibleDescription(
        new RegExp(`Default: ${floor}`),
      );
    });

    it('notes that the presence penalty floor differs for reasoning models', () => {
      renderForm(FLOOR);

      // reasoning_floor() overrides presence_penalty and nothing else, and
      // this surface has no model to check the tag against.
      expect(screen.getByLabelText('Presence Penalty')).toHaveAccessibleDescription(
        /1\.0 for reasoning models/,
      );
    });

    it('claims no default for Max Tokens, because the backend has none', () => {
      renderForm(FLOOR);

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

    it('never states the floor as though it were the answer', () => {
      renderForm(resolved(fromGlobal()));

      // The bug this replaced: a slider parked at 0.70 reading "(default)"
      // while the global setting said 1.2.
      const temperature = screen.getByLabelText('Temperature');
      expect(temperature).not.toHaveAccessibleDescription(/0\.7/);
      expect(temperature).toHaveValue('1.2');
    });

    it('puts the placeholder on the inherited value, not the floor', () => {
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
      const onChange = renderForm(FLOOR);

      await userEvent.type(screen.getByLabelText('Top K'), '5');

      expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ topK: 5 }));
    });

    it('drops the caption once a value is set, and offers a reset', () => {
      renderForm(FLOOR, { topK: 80 });

      expect(screen.getByLabelText('Top K')).toHaveValue(80);
      expect(screen.getByLabelText('Top K')).not.toHaveAccessibleDescription();
      expect(screen.getByRole('button', { name: /Reset Top K to default/ })).toBeInTheDocument();
    });

    it('clears the parameter when reset is pressed', async () => {
      const onChange = renderForm(FLOOR, { topK: 80 });

      await userEvent.click(screen.getByRole('button', { name: /Reset Top K to default/ }));

      expect(onChange).toHaveBeenCalledWith({});
    });
  });
});
