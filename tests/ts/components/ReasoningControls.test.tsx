/**
 * The reasoning controls, and the three answers the GUI can have about
 * whether the effort half applies to a given model.
 *
 * The rule these tests exist to hold: `unknown` is not `no`. gglib reads a
 * template's capabilities from a *running* server, so almost every model in a
 * library answers "never observed" — and a surface that hid the control on
 * that answer would hide it nearly everywhere, which is precisely the
 * unknown-gates mistake ADR 0007 decision 3 forbids the server to make.
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

import { InferenceParametersForm } from '../../../src/components/InferenceParametersForm';
import type { InferenceFallback } from '../../../src/components/InferenceParametersForm/fallbackCaption';
import { ReasoningSupport } from '../../../src/components/ModelInspectorPanel/components/ReasoningSupport';
import { JinjaModeField } from '../../../src/components/ModelInspectorPanel/components/JinjaModeField';
import type { InferenceConfig, TemplateSupport } from '../../../src/types';

const FLOOR: InferenceFallback = { kind: 'floor' };

function renderForm(
  capabilities?: { reasoningEffort: TemplateSupport },
  value?: InferenceConfig,
) {
  const onChange = vi.fn();
  render(
    <InferenceParametersForm
      value={value}
      onChange={onChange}
      fallback={FLOOR}
      capabilities={capabilities}
    />,
  );
  return onChange;
}

describe('the effort control across the three template states', () => {
  it.each<[string, TemplateSupport]>([
    ['a template that reads it', 'yes'],
    ['a template nobody has observed', 'unknown'],
  ])('is offered for %s', (_case, reasoningEffort) => {
    renderForm({ reasoningEffort });

    expect(screen.getByLabelText('Reasoning Effort')).toBeInTheDocument();
  });

  it('is hidden for a template measured not to read it', () => {
    renderForm({ reasoningEffort: 'no' });

    expect(screen.queryByLabelText('Reasoning Effort')).not.toBeInTheDocument();
  });

  it('says why it is hidden rather than simply vanishing', () => {
    // The `ToolSupportIndicator` split: unknown renders nothing, a definite
    // "no" renders something visible. A control that disappears with no
    // explanation reads as a feature gglib does not have.
    renderForm({ reasoningEffort: 'no' });

    expect(screen.getByText(/does not declare reasoning effort/i)).toBeInTheDocument();
  });

  it('never describes an unobserved template as one that refuses', () => {
    renderForm({ reasoningEffort: 'unknown' });

    expect(screen.queryByText(/does not declare reasoning effort/i)).not.toBeInTheDocument();
    expect(screen.getByLabelText('Reasoning Effort')).toHaveAccessibleDescription(
      /not yet observed/i,
    );
  });

  it('tells a surface with no model that the control is conditional', () => {
    // The global settings form edits a default that will meet models on both
    // sides of the capability, so nothing may be hidden there — the caption
    // carries the condition instead.
    renderForm();

    expect(screen.getByLabelText('Reasoning Effort')).toHaveAccessibleDescription(
      /models whose template declares reasoning effort/i,
    );
  });

  it('defaults to offering the control when no capabilities are passed at all', () => {
    renderForm();

    expect(screen.getByLabelText('Reasoning Effort')).toBeInTheDocument();
  });

  it('offers every rung of the ladder and no "none"', async () => {
    renderForm();

    const options = screen
      .getAllByRole('option')
      .map((option) => (option as HTMLOptionElement).value);

    expect(options).toEqual(['', 'minimal', 'low', 'medium', 'high', 'xhigh', 'max']);
    expect(options).not.toContain('none');
  });

  it('sends the chosen level up as a level, not an index', async () => {
    const onChange = renderForm();

    await userEvent.selectOptions(screen.getByLabelText('Reasoning Effort'), 'xhigh');

    expect(onChange).toHaveBeenCalledWith({ reasoningEffort: 'xhigh' });
  });

  it('clears the field back to sending no key at all', async () => {
    const onChange = renderForm(undefined, { reasoningEffort: 'high' });

    await userEvent.selectOptions(screen.getByLabelText('Reasoning Effort'), '');

    // Not `{ reasoningEffort: undefined }` with the key present — an absent
    // key is what leaves the template's own default in place.
    expect(onChange).toHaveBeenCalledWith({});
  });
});

describe('the budget control, which no template can veto', () => {
  it.each<TemplateSupport>(['yes', 'no', 'unknown'])(
    'is offered even when template support is %s',
    (reasoningEffort) => {
      renderForm({ reasoningEffort });

      expect(screen.getByLabelText('Reasoning Budget')).toBeInTheDocument();
    },
  );

  it('accepts the two values that are not counts', () => {
    // -1 defers to the launch default and 0 stops thinking; a field whose
    // floor was 0 or 1 would refuse one of them.
    renderForm();

    expect(screen.getByLabelText('Reasoning Budget')).toHaveAttribute('min', '-1');
  });

  it('states what an empty field falls through to', () => {
    renderForm();

    expect(screen.getByLabelText('Reasoning Budget')).toHaveAccessibleDescription(
      /launch-time budget applies/i,
    );
  });
});

describe('the model inspector fact line', () => {
  const renderSupport = (support: TemplateSupport | undefined, isRunning: boolean) => {
    const onRecheck = vi.fn();
    const onStart = vi.fn();
    render(
      <ReasoningSupport
        support={support}
        isRunning={isRunning}
        onRecheck={onRecheck}
        onStart={onStart}
        isRechecking={false}
      />,
    );
    return { onRecheck, onStart };
  };

  it.each<[TemplateSupport | undefined, RegExp]>([
    ['yes', /reads reasoning effort/i],
    ['no', /does not read reasoning effort/i],
    ['unknown', /not yet observed/i],
    [undefined, /not yet observed/i],
  ])('states %s as itself', (support, pattern) => {
    renderSupport(support, false);

    expect(screen.getByText(pattern)).toBeInTheDocument();
  });

  it('offers a re-measurement, never an override', async () => {
    // There is no "yes it does really" checkbox here on purpose: the answer
    // comes from the renderer executing the template, and a stored opinion
    // that outranked it would be the defect this arc undoes.
    const { onRecheck } = renderSupport('no', true);

    expect(screen.queryByRole('checkbox')).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: /re-check/i }));
    expect(onRecheck).toHaveBeenCalled();
  });

  it('asks for a launch when there is nothing running to read', async () => {
    const { onStart, onRecheck } = renderSupport('unknown', false);

    await userEvent.click(screen.getByRole('button', { name: /start the model/i }));
    expect(onStart).toHaveBeenCalled();
    expect(onRecheck).not.toHaveBeenCalled();
  });
});

describe('the Jinja control, whose three launches a checkbox could not hold', () => {
  const renderJinja = (value: boolean | null, hasAgentTag: boolean) => {
    const onChange = vi.fn();
    const onReset = vi.fn();
    render(
      <JinjaModeField
        value={value}
        hasAgentTag={hasAgentTag}
        disabled={false}
        onChange={onChange}
        onReset={onReset}
      />,
    );
    return { onChange, onReset };
  };

  it('does not call a deferred launch "disabled"', () => {
    // The bug: an untouched control on an untagged model read "Disabled",
    // describing a launch that sends no flag — and llama-server initialises
    // `use_jinja` to true, so that launch runs with Jinja on.
    renderJinja(null, false);

    expect(screen.getByText(/^On — no flag sent/)).toBeInTheDocument();
    expect(screen.queryByText(/disabled/i)).not.toBeInTheDocument();
  });

  it('names llama-server as the source of the deferred answer', () => {
    renderJinja(null, false);

    expect(screen.getByText(/no flag sent/i)).toBeInTheDocument();
  });

  it('distinguishes the agent-tag launch from the deferred one', () => {
    renderJinja(null, true);

    expect(screen.getByText(/agent tag/i)).toBeInTheDocument();
  });

  it.each<[boolean, RegExp]>([
    [true, /--jinja, chosen for this launch/],
    [false, /--no-jinja, chosen for this launch/],
  ])('names the flag an explicit %s sends', (value, pattern) => {
    renderJinja(value, false);

    expect(screen.getByText(pattern)).toBeInTheDocument();
  });

  it('keeps "off" and "let llama-server decide" as separate choices', async () => {
    const { onChange, onReset } = renderJinja(true, false);
    const control = screen.getByLabelText('Jinja Templates');

    await userEvent.selectOptions(control, 'off');
    expect(onChange).toHaveBeenCalledWith(false);

    await userEvent.selectOptions(control, 'auto');
    // Not `onChange(false)` — deferring and disabling are different launches.
    expect(onReset).toHaveBeenCalled();
  });
});
