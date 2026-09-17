import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

import { AdvancedSettings } from '../../../src/components/SettingsModal/fields/AdvancedSettings';
import type { AgentGuardSettingsValues } from '../../../src/components/SettingsModal/useAgentGuardSettings';

const noop = () => {};

const guards = (overrides: Partial<AgentGuardSettingsValues> = {}): AgentGuardSettingsValues => ({
  loopGuardMode: 'note',
  agenticSampling: true,
  toolCallRepair: true,
  maxStagnationSteps: '',
  ...overrides,
});

function renderAdvanced(
  values: AgentGuardSettingsValues,
  setAgentGuardSetting: (key: keyof AgentGuardSettingsValues, value: unknown) => void = noop,
) {
  render(
    <AdvancedSettings
      isOpen
      onToggle={noop}
      maxToolIterationsInput=""
      setMaxToolIterationsInput={noop}
      titlePromptInput=""
      setTitlePromptInput={noop}
      inferenceDefaultsInput={undefined}
      setInferenceDefaultsInput={noop}
      trustClientSampling={false}
      setTrustClientSampling={noop}
      agentGuards={values}
      setAgentGuardSetting={setAgentGuardSetting as never}
      saving={false}
    />,
  );
}

/**
 * The loop guard's control is a three-valued select, not a checkbox: #1052
 * gave the guard a third answer — forward the request with a note — and a
 * boolean cannot say which of the other two a person meant.
 */
describe('the loop-guard mode field', () => {
  it('offers all three modes and shows the one in force', () => {
    renderAdvanced(guards({ loopGuardMode: 'refuse' }));

    const select = screen.getByLabelText(/loop guard on the proxy endpoint/i);
    expect(select).toHaveValue('refuse');
    expect(
      Array.from((select as HTMLSelectElement).options).map((option) => option.value),
    ).toEqual(['note', 'refuse', 'off']);
  });

  it('names note as the default, so the list itself says what unset means', () => {
    renderAdvanced(guards());

    const select = screen.getByLabelText(/loop guard on the proxy endpoint/i) as HTMLSelectElement;
    expect(select).toHaveValue('note');
    expect(select.options[0].textContent).toMatch(/default/i);
  });

  it('reports a change under the setting the update request carries', async () => {
    const setAgentGuardSetting = vi.fn();
    renderAdvanced(guards(), setAgentGuardSetting);

    await userEvent.selectOptions(
      screen.getByLabelText(/loop guard on the proxy endpoint/i),
      'off',
    );

    expect(setAgentGuardSetting).toHaveBeenCalledWith('loopGuardMode', 'off');
  });

  it('no longer renders the boolean it replaced', () => {
    renderAdvanced(guards());

    expect(screen.queryByLabelText(/loop detection on the proxy endpoint/i)).toBeNull();
  });
});
