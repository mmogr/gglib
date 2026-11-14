/**
 * The running launch's fit, as the daemon words it: the sentence, then the
 * figures `gglib model explain` prints, row for row. Nothing is composed
 * here; an image model's launch, and a hub older than the field, show none.
 */

import { describe, it, expect } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import '@testing-library/jest-dom';
import { ProxyLaunchPanel } from '../../../src/places/diagnostics/live/ProxyLaunchPanel';
import type { LaunchNarration } from '../../../src/services/transport/types/dashboard';
import type { LaunchFit } from '../../../src/types/generated/LaunchFit';
import { chatPlan } from '../fixtures/explain';

/** The fixtures' fit as a narration carries it: the same figures, snake_case keys. */
const plan = chatPlan().fit!;
const fit: LaunchFit = {
  serves_at: plan.servesAt,
  source: plan.source,
  fitted: plan.fitted,
  budget_bytes: plan.budgetBytes,
  weights_bytes: plan.weightsBytes,
  kv_bytes_per_token: plan.kvBytesPerToken,
  kv_bytes: plan.kvBytes,
  trained_ctx: plan.trainedCtx,
  unsnapped: plan.unsnapped,
  sentence: plan.sentence,
  note: plan.note,
  figures: plan.figures,
};

function launch(overrides: Partial<LaunchNarration> = {}): LaunchNarration {
  return {
    model_name: 'gemma',
    quantization: 'Q4_K_M',
    weights_bytes: 2_500_000_000,
    decisions: [{ label: 'ctx', value: '32768', source: 'fitted to hardware' }],
    fit,
    ...overrides,
  };
}

describe('ProxyLaunchPanel', () => {
  it("shows the fit's sentence and each figure as served", () => {
    render(<ProxyLaunchPanel launch={launch()} />);
    const group = screen.getByRole('group', { name: 'Context fit' });
    expect(within(group).getByText(fit.sentence)).toBeInTheDocument();
    for (const figure of fit.figures) {
      expect(within(group).getByText(figure.label)).toBeInTheDocument();
      expect(within(group).getByText(figure.value, { selector: '.font-mono' })).toBeInTheDocument();
    }
  });

  it('shows no fit for a launch without one', () => {
    render(<ProxyLaunchPanel launch={launch({ fit: null })} />);
    expect(screen.queryByRole('group', { name: 'Context fit' })).not.toBeInTheDocument();
    expect(screen.getByText('32768')).toBeInTheDocument();
  });

  it('shows no fit from a hub older than the field', () => {
    const { fit: _absent, ...older } = launch();
    render(<ProxyLaunchPanel launch={older as LaunchNarration} />);
    expect(screen.queryByRole('group', { name: 'Context fit' })).not.toBeInTheDocument();
  });
});
