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
const LABEL = /thinking on tool turns/i;

const guards = (overrides: Partial<AgentGuardSettingsValues> = {}): AgentGuardSettingsValues => ({
  loopGuardMode: 'note',
  agenticSampling: true,
  agenticThinking: 'when-asked',
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

describe('the thinking-policy field', () => {
  it('offers the four policies under their wire values and shows the one in force', () => {
    renderAdvanced(guards({ agenticThinking: 'brief' }));

    const select = screen.getByLabelText(LABEL) as HTMLSelectElement;
    expect(select).toHaveValue('brief');
    expect(Array.from(select.options).map((option) => option.value)).toEqual([
      'when-asked',
      'off',
      'brief',
      'default',
    ]);
  });

  it('marks when-asked as the default, and not the policy spelled default', () => {
    renderAdvanced(guards());

    const select = screen.getByLabelText(LABEL) as HTMLSelectElement;
    const labelOf = (value: string) =>
      Array.from(select.options).find((option) => option.value === value)?.textContent ?? '';
    expect(labelOf('when-asked')).toMatch(/\(default\)/i);
    expect(labelOf('default')).not.toMatch(/default/i);
  });

  it('reports a change under the setting the update request carries', async () => {
    const setAgentGuardSetting = vi.fn();
    renderAdvanced(guards(), setAgentGuardSetting);

    await userEvent.selectOptions(screen.getByLabelText(LABEL), 'off');

    expect(setAgentGuardSetting).toHaveBeenCalledWith('agenticThinking', 'off');
  });

  it('is announced with a description that says how a client opts out', () => {
    renderAdvanced(guards());

    const describedBy = screen.getByLabelText(LABEL).getAttribute('aria-describedby');
    expect(describedBy).toBe('agentic-thinking-input-description');
    expect(document.getElementById(describedBy as string)).toHaveTextContent(
      /reasoning_budget_tokens/,
    );
  });

  it('sits beside the agentic sampling cap, after it', () => {
    renderAdvanced(guards());

    const cap = screen.getByLabelText(/agentic sampling cap/i);
    const policy = screen.getByLabelText(LABEL);
    expect(cap.compareDocumentPosition(policy) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });
});

describe('the thinking policy in the agent-guard settings', () => {
  it('reads an unset policy as when-asked and a stored one as itself', () => {
    // Built once: the hook re-seeds whenever it is handed a new object.
    const fresh = appSettings();
    const unset = renderHook(() => useAgentGuardSettings(fresh));
    expect(unset.result.current.values.agenticThinking).toBe('when-asked');

    const unchanged = appSettings({ agenticThinking: 'default' });
    const stored = renderHook(() => useAgentGuardSettings(unchanged));
    expect(stored.result.current.values.agenticThinking).toBe('default');
  });

  it('sends a changed policy alone, and nothing once it is changed back', () => {
    const settings = appSettings({ agenticThinking: 'brief' });
    const { result } = renderHook(() => useAgentGuardSettings(settings));
    expect(result.current.updates).toEqual({});

    act(() => result.current.setValue('agenticThinking', 'off'));
    expect(result.current.updates).toEqual({ agenticThinking: 'off' });

    act(() => result.current.setValue('agenticThinking', 'brief'));
    expect(result.current.updates).toEqual({});
  });

  it('resets to when-asked', () => {
    const settings = appSettings({ agenticThinking: 'off' });
    const { result } = renderHook(() => useAgentGuardSettings(settings));

    act(() => result.current.reset());

    expect(result.current.values.agenticThinking).toBe('when-asked');
    expect(result.current.updates).toEqual({ agenticThinking: 'when-asked' });
  });
});
