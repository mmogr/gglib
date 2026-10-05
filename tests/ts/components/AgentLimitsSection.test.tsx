/**
 * The number rows in the Tools popover, and the property every one of them has
 * to hold: a value in range must be reachable one keystroke at a time.
 *
 * These fields are controlled inputs whose `onChange` decides what the next
 * render shows. That makes any bound checked per keystroke a bound on prefixes
 * rather than on answers — "3" and "30" are below the tool timeout's floor of
 * 100 on the way to a perfectly legal 30000, and a handler that answers them
 * with `undefined` blanks the field and makes the row impossible to type into.
 * The rows floored at 1 cannot catch that regression, because every prefix of
 * their values is already in range; only the timeout row can, which is why it
 * is tested here beside the reasoning rows this arc added.
 *
 * The effort row is the settings surfaces' three-state field: a note in place
 * of the dropdown where the chat's model was observed not to read a level, the
 * dropdown everywhere else, and the budget row under it whatever the answer.
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

import { AgentLimitsSection } from '../../../src/components/ToolsPopover/AgentLimitsSection';
import {
  TOOL_TIMEOUT_MS_FLOOR,
  TOOL_TIMEOUT_MS_CEILING,
  readStoredAgentOverrides,
} from '../../../src/services/agentOverrides';

const timeoutField = () => screen.getByLabelText('Tool timeout (ms)');
const budgetField = () => screen.getByLabelText('Reasoning budget');

describe('typing a value into a row whose floor is above 1', () => {
  beforeEach(() => localStorage.clear());

  it('reaches a five-digit timeout through its below-floor prefixes', async () => {
    // The regression this file exists for. Every prefix of 30000 up to "30" is
    // under TOOL_TIMEOUT_MS_FLOOR; a per-keystroke floor test turns each of
    // them into `undefined`, React restores the DOM node to '', and the field
    // never holds anything at all.
    render(<AgentLimitsSection />);

    await userEvent.type(timeoutField(), '30000');

    expect(timeoutField()).toHaveValue(30000);
    expect(readStoredAgentOverrides().toolTimeoutMs).toBe(30000);
  });

  it('shows each prefix as it is typed instead of swallowing it', async () => {
    render(<AgentLimitsSection />);
    const field = timeoutField();

    await userEvent.type(field, '3');
    expect(field).toHaveValue(3);

    await userEvent.type(field, '0');
    expect(field).toHaveValue(30);
  });

  it('lifts a below-floor value to the floor once the field is done', async () => {
    // The bound is not abandoned, only moved to where it can tell a finished
    // answer from a prefix of one.
    render(<AgentLimitsSection />);

    await userEvent.type(timeoutField(), '3');
    await userEvent.tab();

    expect(timeoutField()).toHaveValue(TOOL_TIMEOUT_MS_FLOOR);
    expect(readStoredAgentOverrides().toolTimeoutMs).toBe(TOOL_TIMEOUT_MS_FLOOR);
  });

  it('holds an above-ceiling value down to the ceiling on the same commit', async () => {
    render(<AgentLimitsSection />);

    await userEvent.type(timeoutField(), '999999');
    await userEvent.tab();

    expect(timeoutField()).toHaveValue(TOOL_TIMEOUT_MS_CEILING);
  });

  it('leaves a value already in range untouched by the commit', async () => {
    render(<AgentLimitsSection />);

    await userEvent.type(timeoutField(), '30000');
    await userEvent.tab();

    expect(timeoutField()).toHaveValue(30000);
  });

  it('clears the override when the field is emptied', async () => {
    render(<AgentLimitsSection />);

    await userEvent.type(timeoutField(), '30000');
    await userEvent.clear(timeoutField());
    await userEvent.tab();

    expect(timeoutField()).toHaveValue(null);
    // Not the floor: an empty field means "no override", and clamping an
    // absence into a number would send a limit the user never chose.
    expect(readStoredAgentOverrides().toolTimeoutMs).toBeUndefined();
  });
});

describe('the reasoning budget, whose legal values are not counts', () => {
  beforeEach(() => localStorage.clear());

  it('accepts 0, which means stop thinking', async () => {
    render(<AgentLimitsSection />);

    await userEvent.type(budgetField(), '0');
    await userEvent.tab();

    expect(budgetField()).toHaveValue(0);
    expect(readStoredAgentOverrides().reasoningBudgetTokens).toBe(0);
  });

  it('accepts -1, which defers to the launch default', async () => {
    render(<AgentLimitsSection />);

    await userEvent.type(budgetField(), '-1');
    await userEvent.tab();

    expect(budgetField()).toHaveValue(-1);
    expect(readStoredAgentOverrides().reasoningBudgetTokens).toBe(-1);
  });

  it('keeps a plain count as a plain count', async () => {
    render(<AgentLimitsSection />);

    await userEvent.type(budgetField(), '1024');
    await userEvent.tab();

    expect(budgetField()).toHaveValue(1024);
    expect(readStoredAgentOverrides().reasoningBudgetTokens).toBe(1024);
  });

  it('lifts a value below -1 to the floor the server validates against', async () => {
    render(<AgentLimitsSection />);

    await userEvent.type(budgetField(), '-5');
    await userEvent.tab();

    expect(budgetField()).toHaveValue(-1);
  });
});

describe('the reasoning effort row, by what is known of the model\'s template', () => {
  beforeEach(() => localStorage.clear());

  const effortField = () => screen.queryByRole('combobox', { name: 'Reasoning Effort' });
  const NOTE = /template does not declare reasoning effort/;

  it('shows a note and no dropdown where the template reads no level', () => {
    render(<AgentLimitsSection effortSupport="no" />);

    expect(effortField()).not.toBeInTheDocument();
    expect(screen.queryByRole('combobox')).not.toBeInTheDocument();
    expect(screen.getByText(NOTE)).toBeInTheDocument();
  });

  it.each([
    ['unknown', 'Not yet observed — start the model to find out whether its template reads this. Until then a level set here is sent as given.'],
    ['yes', "This model's template reads reasoning effort, so a level set here is honoured."],
  ] as const)('keeps the dropdown where support is %s, and says so', (support, caption) => {
    render(<AgentLimitsSection effortSupport={support} />);

    expect(effortField()).toBeInTheDocument();
    expect(effortField()).toHaveAccessibleDescription(caption);
    expect(screen.queryByText(NOTE)).not.toBeInTheDocument();
  });

  it('keeps the dropdown with no model in scope, and names the condition rather than a model', () => {
    render(<AgentLimitsSection />);

    expect(effortField()).toBeInTheDocument();
    expect(effortField()).toHaveAccessibleDescription(
      'Applies to models whose template declares reasoning effort; others ignore it.',
    );
    expect(screen.queryByText(NOTE)).not.toBeInTheDocument();
  });

  it.each([['no'], ['unknown'], ['yes'], [undefined]] as const)(
    'always has the budget row, here with support %s',
    async (support) => {
      render(<AgentLimitsSection effortSupport={support} />);

      await userEvent.type(budgetField(), '512');
      await userEvent.tab();
      expect(budgetField()).toHaveValue(512);
      expect(readStoredAgentOverrides().reasoningBudgetTokens).toBe(512);
    },
  );

  it('stores a level picked from the dropdown, and clears it on the blank choice', async () => {
    render(<AgentLimitsSection effortSupport="unknown" />);

    await userEvent.selectOptions(effortField()!, 'high');
    expect(effortField()).toHaveValue('high');
    expect(readStoredAgentOverrides().reasoningEffort).toBe('high');

    await userEvent.selectOptions(effortField()!, '');
    expect(readStoredAgentOverrides().reasoningEffort).toBeUndefined();
  });

  it('shows a level stored before, and leaves it stored where the note replaces the dropdown', () => {
    localStorage.setItem('gglib.chat.agentOverrides', JSON.stringify({ reasoningEffort: 'low' }));
    const seen = render(<AgentLimitsSection effortSupport="yes" />);
    expect(effortField()).toHaveValue('low');
    seen.unmount();

    render(<AgentLimitsSection effortSupport="no" />);
    expect(readStoredAgentOverrides().reasoningEffort).toBe('low');
  });

  it('says the budget is for every chat on this device, and that a chat with Thinking switched off runs with none', () => {
    render(<AgentLimitsSection />);

    expect(
      screen.getByText(
        'The budget is a hard cap llama.cpp enforces on any model, in every chat on this device. A chat with Thinking switched off runs with a budget of 0, whatever is set here.',
      ),
    ).toBeInTheDocument();
  });
});
