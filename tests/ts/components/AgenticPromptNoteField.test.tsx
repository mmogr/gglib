import { describe, it, expect, vi } from 'vitest';
import { act, render, renderHook, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

import { AdvancedSettings } from '../../../src/components/SettingsModal/fields/AdvancedSettings';
import {
  useAgentGuardSettings,
  type AgentGuardSettingsValues,
} from '../../../src/components/SettingsModal/useAgentGuardSettings';
import { appSettings } from '../fixtures/settings';

// The Advanced section renders the loop guard's log, which reads it on mount;
// this file is about one field, so the read answers with nothing.
vi.mock('../../../src/services/transport/api/proxy', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../src/services/transport/api/proxy')>()),
  getLoopGuardTrips: vi.fn().mockResolvedValue([]),
}));

const noop = () => {};
const LABEL = /batching note on tool turns/i;

const guards = (overrides: Partial<AgentGuardSettingsValues> = {}): AgentGuardSettingsValues => ({
  loopGuardMode: 'note',
  agenticSampling: true,
  agenticThinking: 'when-asked',
  agenticPromptNote: false,
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

describe('the batching-note field', () => {
  it('shows the switch in the position it is given', () => {
    renderAdvanced(guards());
    expect(screen.getByLabelText(LABEL)).not.toBeChecked();
  });

  it('shows a stored note as on', () => {
    renderAdvanced(guards({ agenticPromptNote: true }));
    expect(screen.getByLabelText(LABEL)).toBeChecked();
  });

  it('reports a change under the setting the update request carries', async () => {
    const setAgentGuardSetting = vi.fn();
    renderAdvanced(guards(), setAgentGuardSetting);

    await userEvent.click(screen.getByLabelText(LABEL));

    expect(setAgentGuardSetting).toHaveBeenCalledWith('agenticPromptNote', true);
  });

  it('says it is off by default and not measured', () => {
    renderAdvanced(guards());

    expect(screen.getByText(/off by default, and not measured/i)).toBeInTheDocument();
  });

  it('sits after the thinking policy', () => {
    renderAdvanced(guards());

    const policy = screen.getByLabelText(/thinking on tool turns/i);
    const note = screen.getByLabelText(LABEL);
    expect(policy.compareDocumentPosition(note) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });
});

describe('the batching note in the agent-guard settings', () => {
  it('reads an unset or false note as off and a stored true as on', () => {
    // Built once: the hook re-seeds whenever it is handed a new object.
    const fresh = appSettings();
    const unset = renderHook(() => useAgentGuardSettings(fresh));
    expect(unset.result.current.values.agenticPromptNote).toBe(false);

    const off = appSettings({ agenticPromptNote: false });
    const storedOff = renderHook(() => useAgentGuardSettings(off));
    expect(storedOff.result.current.values.agenticPromptNote).toBe(false);

    const on = appSettings({ agenticPromptNote: true });
    const storedOn = renderHook(() => useAgentGuardSettings(on));
    expect(storedOn.result.current.values.agenticPromptNote).toBe(true);
  });

  it('sends a changed note alone, and nothing once it is changed back', () => {
    const settings = appSettings();
    const { result } = renderHook(() => useAgentGuardSettings(settings));
    expect(result.current.updates).toEqual({});

    act(() => result.current.setValue('agenticPromptNote', true));
    expect(result.current.updates).toEqual({ agenticPromptNote: true });

    act(() => result.current.setValue('agenticPromptNote', false));
    expect(result.current.updates).toEqual({});
  });

  it('resets to off', () => {
    const settings = appSettings({ agenticPromptNote: true });
    const { result } = renderHook(() => useAgentGuardSettings(settings));

    act(() => result.current.reset());

    expect(result.current.values.agenticPromptNote).toBe(false);
    expect(result.current.updates).toEqual({ agenticPromptNote: false });
  });
});
