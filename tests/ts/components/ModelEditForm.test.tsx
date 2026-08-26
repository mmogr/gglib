/**
 * The context-length override's two-state toggle.
 *
 * The button beside the field means one of two things depending on where the
 * override currently stands, and it has to be able to get back from either:
 *
 * - an object → ✕ "Clear override", which sets the state to `null`
 * - `null`    → ↺ "Revert 'clear' action", which puts the model's stored
 *   value back
 *
 * The revert arm is the fragile one. Most models have no stored
 * `serverDefaults` at all, so the value it reverts *to* is a fallback rather
 * than the model's own — and if that fallback is itself `null`, pressing the
 * button changes nothing, the icon never flips back, and `handleSave` goes on
 * to persist the clear the user just tried to undo.
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

import { ModelEditForm } from '../../../src/components/ModelInspectorPanel/components/ModelEditForm';
import { guiModel } from '../fixtures/model';

vi.mock('../../../src/components/InferenceParametersForm', () => ({
  InferenceParametersForm: () => null,
}));

// The hook lives under the panel, not in `src/hooks`. A mock aimed at a
// module that does not exist registers nothing and fails silently, and the
// component then runs the real hook — which reaches the transport.
vi.mock('../../../src/components/ModelInspectorPanel/hooks/useSamplingExplanation', () => ({
  useSamplingExplanation: () => ({ explanation: null, isLoading: false, hasError: false }),
}));

function renderForm(overrides: Parameters<typeof guiModel>[0], edited: unknown) {
  const onServerDefaultsChange = vi.fn();
  const model = guiModel(overrides);
  render(
    <ModelEditForm
      model={model}
      editedQuantization={model.quantization ?? ''}
      editedFilePath={model.filePath}
      editedInferenceDefaults={undefined}
      editedServerDefaults={edited as null}
      onQuantizationChange={vi.fn()}
      onFilePathChange={vi.fn()}
      onInferenceDefaultsChange={vi.fn()}
      onServerDefaultsChange={onServerDefaultsChange}
    />,
  );
  return onServerDefaultsChange;
}

describe('ModelEditForm context-length override', () => {
  it('clears the override to null when the ✕ is pressed', async () => {
    const onServerDefaultsChange = renderForm({}, { contextLength: 8192 });

    await userEvent.click(screen.getByRole('button', { name: 'Clear override' }));

    expect(onServerDefaultsChange).toHaveBeenCalledWith(null);
  });

  /**
   * The regression this file exists for. A model with nothing stored is the
   * ordinary case, and reverting used to hand back an empty object — a
   * non-null value, which is what flips the button's state. Handing back
   * `null` instead leaves it exactly where it was.
   */
  it('reverts to a non-null override on a model with nothing stored', async () => {
    const onServerDefaultsChange = renderForm({ serverDefaults: undefined }, null);

    await userEvent.click(screen.getByRole('button', { name: "Revert 'clear' action" }));

    expect(onServerDefaultsChange).toHaveBeenCalledTimes(1);
    expect(onServerDefaultsChange.mock.calls[0][0]).not.toBeNull();
  });

  it("reverts to the model's own value when it has one", async () => {
    const onServerDefaultsChange = renderForm({ serverDefaults: { contextLength: 4096 } }, null);

    await userEvent.click(screen.getByRole('button', { name: "Revert 'clear' action" }));

    expect(onServerDefaultsChange).toHaveBeenCalledWith({ contextLength: 4096 });
  });

  it('says an empty box is no override, not a default', () => {
    // This field writes `serverDefaults.contextLength`, which IS a rung of the
    // ladder — so "Use default" was a fourth sense of the word on one panel,
    // and the wrong one: an empty box here means the model states no override,
    // not that some default applies. See ADR 0009.
    renderForm({ serverDefaults: undefined }, undefined);
    expect(screen.getByPlaceholderText('No override')).toBeInTheDocument();
    expect(screen.queryByPlaceholderText(/default/i)).toBeNull();
  });

  it('offers no button at all while the override is untouched', () => {
    renderForm({}, undefined);

    expect(screen.queryByRole('button', { name: 'Clear override' })).not.toBeInTheDocument();
    expect(
      screen.queryByRole('button', { name: "Revert 'clear' action" }),
    ).not.toBeInTheDocument();
  });
});
