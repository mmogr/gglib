import { describe, it, expect, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

import { PortSettings } from '../../../src/components/SettingsModal/fields/PortSettings';
import { ModelDefaults } from '../../../src/components/SettingsModal/fields/ModelDefaults';
import { AdvancedSettings } from '../../../src/components/SettingsModal/fields/AdvancedSettings';

const noop = () => {};

type Setter = (value: string) => void;

/** Renders the section owning a field, with that one field's value and setter. */
type RenderField = (value: string, onChange: Setter) => void;

const renderPortSettings =
  (field: 'proxy' | 'server' | 'queue'): RenderField =>
  (value, onChange) =>
    render(
      <PortSettings
        proxyPortInput={field === 'proxy' ? value : ''}
        setProxyPortInput={field === 'proxy' ? onChange : noop}
        serverPortInput={field === 'server' ? value : ''}
        setServerPortInput={field === 'server' ? onChange : noop}
        maxQueueSizeInput={field === 'queue' ? value : ''}
        setMaxQueueSizeInput={field === 'queue' ? onChange : noop}
        saving={false}
      />,
    );

const renderModelDefaults: RenderField = (value, onChange) =>
  render(
    <ModelDefaults
      contextSizeInput={value}
      setContextSizeInput={onChange}
      defaultModelInput=""
      setDefaultModelInput={noop}
      models={[]}
      loadingModels={false}
      saving={false}
    />,
  );

const renderAdvanced = (
  overrides: Partial<{ maxToolIterationsInput: string; stagnation: string }>,
  onChange: (value: string) => void,
  target: 'iterations' | 'stagnation',
) =>
  render(
    <AdvancedSettings
      isOpen
      onToggle={noop}
      maxToolIterationsInput={overrides.maxToolIterationsInput ?? ''}
      setMaxToolIterationsInput={target === 'iterations' ? onChange : noop}
      titlePromptInput=""
      setTitlePromptInput={noop}
      inferenceDefaultsInput={undefined}
      setInferenceDefaultsInput={noop}
      trustClientSampling={false}
      setTrustClientSampling={noop}
      proxyLoopDetection
      setProxyLoopDetection={noop}
      agentGuards={{ agenticSampling: true, maxStagnationSteps: overrides.stagnation ?? '' }}
      setAgentGuardSetting={(key, value) => {
        if (target === 'stagnation' && key === 'maxStagnationSteps') onChange(value as string);
      }}
      saving={false}
    />,
  );

const renderAdvancedSettings: RenderField = (value, onChange) =>
  renderAdvanced({ maxToolIterationsInput: value }, onChange, 'iterations');

const renderStagnationSettings: RenderField = (value, onChange) =>
  renderAdvanced({ stagnation: value }, onChange, 'stagnation');

/**
 * Every numeric field in the settings modal, with the values a user should
 * see.
 *
 * The expectations are literals rather than imports from
 * `src/constants/settingsDefaults.ts` on purpose: a test that reads the same
 * constant it is checking would pass whatever that constant happened to say,
 * including a typo. These are transcribed from
 * `crates/gglib-core/src/settings.rs`, which is what the GUI is promising the
 * user the backend will do.
 */
const FIELDS: {
  label: string;
  fallback: string;
  min: string;
  max: string;
  renderField: RenderField;
}[] = [
  {
    label: 'Proxy Server Port',
    fallback: '8080',
    min: '1024',
    max: '65535',
    renderField: renderPortSettings('proxy'),
  },
  {
    label: 'Base Server Port',
    fallback: '9000',
    min: '1024',
    max: '65535',
    renderField: renderPortSettings('server'),
  },
  {
    label: 'Max Download Queue Size',
    fallback: '10',
    min: '1',
    max: '50',
    renderField: renderPortSettings('queue'),
  },
  {
    label: 'Default Context Size',
    fallback: '4096',
    min: '512',
    max: '1000000',
    renderField: renderModelDefaults,
  },
  {
    label: 'Max Tool Iterations',
    fallback: '25',
    min: '1',
    max: '50',
    renderField: renderAdvancedSettings,
  },
  {
    label: 'Max Stagnation Steps',
    fallback: '5',
    min: '1',
    max: '100',
    renderField: renderStagnationSettings,
  },
];

describe.each(FIELDS)('$label', ({ label, fallback, min, max, renderField }) => {
  it('offers the default as a placeholder to type over while empty', () => {
    renderField('', noop);

    // The regression from #730: the hint below the field was left in place
    // but the in-box affordance went away.
    expect(screen.getByLabelText(label)).toHaveAttribute('placeholder', fallback);
  });

  it('states the default below the field', () => {
    renderField('', noop);

    expect(screen.getByText(`Default: ${fallback}`)).toBeInTheDocument();
  });

  it('keeps the default legible once a value is set', async () => {
    renderField('7', noop);

    // The placeholder is gone at this point — the hint is the only thing
    // still telling the user what clearing the field would fall back to.
    expect(screen.getByLabelText(label)).toHaveValue(7);
    expect(screen.getByText(`Default: ${fallback}`)).toBeInTheDocument();
  });

  it('announces the default to assistive technology', () => {
    renderField('', noop);

    expect(screen.getByLabelText(label)).toHaveAccessibleDescription(
      new RegExp(`Default: ${fallback}`),
    );
  });

  it('constrains input to the range the backend accepts', () => {
    renderField('', noop);

    const input = screen.getByLabelText(label);
    expect(input).toHaveAttribute('type', 'number');
    expect(input).toHaveAttribute('min', min);
    expect(input).toHaveAttribute('max', max);
  });

  it('reports edits to its own setter', async () => {
    const onChange = vi.fn();
    renderField('', onChange);

    await userEvent.type(screen.getByLabelText(label), '7');

    expect(onChange).toHaveBeenCalledWith('7');
  });
});
