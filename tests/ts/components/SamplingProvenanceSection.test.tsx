import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';

import { SamplingProvenanceSection } from '../../../src/components/ModelInspectorPanel/components/SamplingProvenanceSection';
import {
  caveats,
  describeSource,
  formatParamValue,
} from '../../../src/components/ModelInspectorPanel/components/samplingProvenance';
import type {
  InferenceProfile,
  ParamProvenance,
  SamplingExplanation,
} from '../../../src/types';

const explainModelSampling = vi.fn();

vi.mock('../../../src/services/transport/api/models/local', () => ({
  explainModelSampling: (...args: unknown[]) => explainModelSampling(...args),
}));

/** The seven entries the server always sends, in its display order. */
function sources(overrides: Partial<Record<string, ParamProvenance>> = {}): ParamProvenance[] {
  const base: ParamProvenance[] = [
    { param: 'temperature', kind: 'layer', layer: 'profile' },
    { param: 'topP', kind: 'floor' },
    { param: 'topK', kind: 'layer', layer: 'global' },
    { param: 'presencePenalty', kind: 'floorCoupled' },
    { param: 'repeatPenalty', kind: 'floor' },
    { param: 'minP', kind: 'layer', layer: 'modelAutoDetected' },
    { param: 'maxTokens', kind: 'unset' },
  ];
  return base.map((entry) => overrides[entry.param] ?? entry);
}

function explanation(overrides: Partial<SamplingExplanation> = {}): SamplingExplanation {
  return {
    resolved: {
      temperature: 0.2,
      topP: 0.95,
      topK: 40,
      presencePenalty: 0,
      repeatPenalty: 1,
      minP: 0.05,
    },
    sources: sources(),
    profile: 'coding',
    isReasoning: false,
    trustClientSampling: false,
    ...overrides,
  };
}

function profile(name: string): InferenceProfile {
  return { name, description: null, config: {}, listInModels: false };
}

beforeEach(() => {
  explainModelSampling.mockReset();
  explainModelSampling.mockResolvedValue(explanation());
});

describe('SamplingProvenanceSection', () => {
  it('renders every parameter with its value and the layer that supplied it', async () => {
    render(<SamplingProvenanceSection modelId={1} profiles={[]} />);

    expect(await screen.findByText('Temperature')).toBeInTheDocument();
    expect(screen.getByText('0.2')).toBeInTheDocument();
    expect(screen.getByText("profile 'coding'")).toBeInTheDocument();
    expect(screen.getByText('global settings')).toBeInTheDocument();
    expect(
      screen.getByText('per-model defaults (auto-detected: reasoning tag)'),
    ).toBeInTheDocument();
  });

  /// A whole float keeps its decimal so it reads as a sampling parameter.
  it('formats whole floats with a decimal place and absent values as a dash', async () => {
    render(<SamplingProvenanceSection modelId={1} profiles={[]} />);

    expect(await screen.findByText('1.0')).toBeInTheDocument();
    expect(screen.getByText('—')).toBeInTheDocument();
    expect(screen.getByText('unset by design')).toBeInTheDocument();
  });

  it('names the reasoning floor and the coupling that reached it', async () => {
    explainModelSampling.mockResolvedValue(explanation({ isReasoning: true }));
    render(<SamplingProvenanceSection modelId={1} profiles={[]} />);

    expect(
      await screen.findByText('reasoning floor (coupled to temperature layer)'),
    ).toBeInTheDocument();
    expect(screen.getAllByText('reasoning floor').length).toBeGreaterThan(0);
  });

  it('states that operator flags outrank the table, and whether clients are trusted', async () => {
    render(<SamplingProvenanceSection modelId={1} profiles={[]} />);

    expect(await screen.findByText(/Operator flags/)).toBeInTheDocument();
    expect(screen.getByText(/Client-supplied sampling is ignored/)).toBeInTheDocument();

    explainModelSampling.mockResolvedValue(explanation({ trustClientSampling: true }));
    render(<SamplingProvenanceSection modelId={2} profiles={[]} />);

    expect(await screen.findByText(/Client-supplied sampling is trusted/)).toBeInTheDocument();
  });

  it('offers no profile selector when none are configured', async () => {
    render(<SamplingProvenanceSection modelId={1} profiles={[]} />);

    await screen.findByText('Temperature');
    expect(screen.queryByLabelText('Inference profile')).not.toBeInTheDocument();
  });

  it('re-resolves against the selected profile', async () => {
    render(<SamplingProvenanceSection modelId={1} profiles={[profile('coding')]} />);

    const select = await screen.findByLabelText('Inference profile');
    expect(explainModelSampling).toHaveBeenCalledWith(1, undefined);

    await userEvent.selectOptions(select, 'coding');

    await waitFor(() => expect(explainModelSampling).toHaveBeenCalledWith(1, 'coding'));
  });

  it('falls back to a notice rather than stale rows when the fetch fails', async () => {
    explainModelSampling.mockRejectedValue(new Error('boom'));
    render(<SamplingProvenanceSection modelId={1} profiles={[]} />);

    expect(await screen.findByText('Sampling provenance unavailable.')).toBeInTheDocument();
    expect(screen.queryByText('Temperature')).not.toBeInTheDocument();
  });
});

describe('describeSource', () => {
  const ctx = { profile: 'coding', isReasoning: false };

  it('matches the wording gglib model explain prints', () => {
    expect(describeSource({ param: 'temperature', kind: 'layer', layer: 'request' }, ctx)).toBe(
      'request parameters',
    );
    expect(describeSource({ param: 'temperature', kind: 'layer', layer: 'profile' }, ctx)).toBe(
      "profile 'coding'",
    );
    expect(
      describeSource({ param: 'temperature', kind: 'layer', layer: 'modelUserSet' }, ctx),
    ).toBe('per-model defaults (user-set)');
    expect(describeSource({ param: 'temperature', kind: 'layer', layer: 'global' }, ctx)).toBe(
      'global settings',
    );
    expect(describeSource({ param: 'temperature', kind: 'unset' }, ctx)).toBe('unset by design');
  });

  it('names the profile rung without a name when none was selected', () => {
    expect(
      describeSource({ param: 'temperature', kind: 'layer', layer: 'profile' }, {
        profile: null,
        isReasoning: false,
      }),
    ).toBe('profile');
  });

  it('switches the floor name on the reasoning tag', () => {
    expect(describeSource({ param: 'topP', kind: 'floor' }, ctx)).toBe('default floor');
    expect(
      describeSource({ param: 'topP', kind: 'floor' }, { profile: null, isReasoning: true }),
    ).toBe('reasoning floor');
  });

  /// A layer the server could not name is still worth showing.
  it('renders an unnamed layer visibly rather than dropping it', () => {
    expect(describeSource({ param: 'temperature', kind: 'layer' }, ctx)).toBe('unknown layer');
  });
});

describe('formatParamValue', () => {
  it('keeps a decimal on whole floats but not on counts', () => {
    expect(formatParamValue('temperature', 1)).toBe('1.0');
    expect(formatParamValue('temperature', 0.65)).toBe('0.65');
    expect(formatParamValue('topK', 40)).toBe('40');
    expect(formatParamValue('maxTokens', 32768)).toBe('32,768');
  });

  it('renders an absent value as the shared unknown placeholder', () => {
    expect(formatParamValue('maxTokens', undefined)).toBe('—');
    expect(formatParamValue('minP', null)).toBe('—');
  });
});

describe('caveats', () => {
  it('always leads with the operator flags that outrank every stored layer', () => {
    expect(caveats(false)[0]).toMatch(/Operator flags/);
    expect(caveats(true)[0]).toMatch(/Operator flags/);
  });

  it('reports the client-sampling posture from settings', () => {
    expect(caveats(false)[1]).toMatch(/ignored, except max_tokens/);
    expect(caveats(true)[1]).toMatch(/trusted/);
  });
});
