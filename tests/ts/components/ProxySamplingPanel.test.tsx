import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import ProxySamplingPanel, { formatValue } from '../../../src/components/ProxySamplingPanel';
import type { SamplingAuditSnapshot } from '../../../src/services/transport/types/dashboard';

function audit(overrides: Partial<SamplingAuditSnapshot> = {}): SamplingAuditSnapshot {
  return {
    state: { state: 'not_yet_observed' },
    skipped_ambiguous: 0,
    client_fields_rejected: 0,
    client_fields_discarded: 0,
    recent_divergences: [],
    baseline: { state: 'not_yet_read' },
    ...overrides,
  };
}

describe('ProxySamplingPanel', () => {
  // The rule the whole Tier C contract rests on. A blind organ and a healthy
  // one both report zero divergences and mean opposite things; the backend
  // keeps them apart as distinct states, and collapsing them here would throw
  // that away at the last step.
  it('renders a blind readback as a warning with its cause, never as health', () => {
    render(
      <ProxySamplingPanel
        audit={audit({
          state: {
            state: 'blind',
            reason: 'llama-server was launched with --no-slots, so nothing can be read back.',
          },
        })}
      />,
    );

    expect(screen.getByText(/readback is blind/i)).toBeInTheDocument();
    expect(screen.getByText(/--no-slots/)).toBeInTheDocument();
    expect(screen.queryByText('Requests observed')).not.toBeInTheDocument();
    expect(screen.queryByText('Diverged')).not.toBeInTheDocument();
  });

  it('renders a clean readback with its counts, distinctly from blind', () => {
    render(
      <ProxySamplingPanel
        audit={audit({ state: { state: 'comparing', comparisons: 412, divergences: 0 } })}
      />,
    );

    expect(screen.getByText('Requests observed')).toBeInTheDocument();
    expect(screen.getByText('412')).toBeInTheDocument();
    expect(screen.getByText('Diverged')).toBeInTheDocument();
    expect(screen.queryByText(/readback is blind/i)).not.toBeInTheDocument();
  });

  it('says nothing has been caught yet rather than reporting a zero', () => {
    render(<ProxySamplingPanel audit={audit()} />);
    expect(screen.getByText(/no request caught mid-generation yet/i)).toBeInTheDocument();
    expect(screen.queryByText('Diverged')).not.toBeInTheDocument();
  });

  it('lists a divergence with what was sent, what was seen, and where it came from', () => {
    render(
      <ProxySamplingPanel
        audit={audit({
          state: { state: 'comparing', comparisons: 10, divergences: 1 },
          recent_divergences: [
            { field: 'temperature', sent: 0.7, observed: 1.5, provenance: 'profile' },
          ],
        })}
      />,
    );

    expect(screen.getByText('temperature')).toBeInTheDocument();
    expect(screen.getByText(/sent 0\.7 · server reports 1\.5/)).toBeInTheDocument();
    expect(screen.getByText('(profile)')).toBeInTheDocument();
  });

  // Abstention is something a *sighted* organ does, so it must not read as a
  // failure — but it must be visible, because a high count means the traffic
  // cannot be attributed and no comparison is happening.
  it('surfaces unattributable polls only when there are some', () => {
    const { rerender } = render(
      <ProxySamplingPanel
        audit={audit({ state: { state: 'comparing', comparisons: 5, divergences: 0 } })}
      />,
    );
    expect(screen.queryByText('Unattributable')).not.toBeInTheDocument();

    rerender(
      <ProxySamplingPanel
        audit={audit({
          state: { state: 'comparing', comparisons: 5, divergences: 0 },
          skipped_ambiguous: 31,
        })}
      />,
    );
    expect(screen.getByText('Unattributable')).toBeInTheDocument();
    expect(screen.getByText('31')).toBeInTheDocument();
  });

  describe('baseline check', () => {
    // A field this build's /props does not report is unknown, never
    // agreement — the same discipline as `RuntimeCapabilities::unknown`.
    it('renders an inconclusive baseline as unknown, not as a match', () => {
      render(
        <ProxySamplingPanel
          audit={audit({
            baseline: {
              state: 'read',
              report: {
                coverage: { coverage: 'blind', indeterminate: 1 },
                fields: [
                  {
                    field: 'temperature',
                    verdict: {
                      verdict: 'indeterminate',
                      reason: 'this build\u2019s /props does not report temperature',
                    },
                  },
                ],
              },
            },
          })}
        />,
      );

      expect(screen.getByText(/could not be checked/i)).toBeInTheDocument();
      expect(screen.getByText(/does not report temperature/i)).toBeInTheDocument();
      expect(screen.queryByText(/match the values/i)).not.toBeInTheDocument();
    });

    // ADR 0003's reverse deletion criterion firing: a pin bump moved a default
    // gglib defers to.
    it('raises drift as a danger with both numbers', () => {
      render(
        <ProxySamplingPanel
          audit={audit({
            baseline: {
              state: 'read',
              report: {
                coverage: { coverage: 'complete' },
                fields: [
                  { field: 'top_p', verdict: { verdict: 'differs', expected: 0.95, observed: 0.9 } },
                  { field: 'min_p', verdict: { verdict: 'matches' } },
                ],
              },
            },
          })}
        />,
      );

      expect(screen.getByText(/defaults have moved/i)).toBeInTheDocument();
      expect(
        screen.getByText(/top_p: expected 0\.95, this build reports 0\.9/),
      ).toBeInTheDocument();
    });

    it('reports a clean conclusive baseline plainly', () => {
      render(
        <ProxySamplingPanel
          audit={audit({
            baseline: {
              state: 'read',
              report: {
                coverage: { coverage: 'complete' },
                fields: [
                  { field: 'top_p', verdict: { verdict: 'matches' } },
                  { field: 'min_p', verdict: { verdict: 'matches' } },
                ],
              },
            },
          })}
        />,
      );

      expect(screen.getByText(/All 2 sampler defaults match/i)).toBeInTheDocument();
    });

    // **The defect.** `conclusive` was "any field reached a verdict", so two
    // of seven checked rendered as an all-clear over all seven — the panel's
    // own blind-as-health rule, applied to the report instead of to a field.
    it('never reports an all-clear when only some fields could be checked', () => {
      render(
        <ProxySamplingPanel
          audit={audit({
            baseline: {
              state: 'read',
              report: {
                coverage: { coverage: 'partial', checked: 2, indeterminate: 1 },
                fields: [
                  { field: 'top_p', verdict: { verdict: 'matches' } },
                  { field: 'min_p', verdict: { verdict: 'matches' } },
                  {
                    field: 'temperature',
                    verdict: { verdict: 'indeterminate', reason: 'not reported by this build' },
                  },
                ],
              },
            },
          })}
        />,
      );

      expect(screen.queryByText(/All \d+ sampler defaults match/i)).not.toBeInTheDocument();
      expect(screen.getByText(/Checked 2 of 3 sampler defaults/i)).toBeInTheDocument();
      expect(screen.getByText(/not reported by this build/i)).toBeInTheDocument();
    });

    // Coverage is checked after drift, so a partial reading that found a moved
    // default still raises the alarm rather than being softened into "some
    // fields could not be checked".
    it('still raises drift when coverage is only partial', () => {
      render(
        <ProxySamplingPanel
          audit={audit({
            baseline: {
              state: 'read',
              report: {
                coverage: { coverage: 'partial', checked: 1, indeterminate: 1 },
                fields: [
                  { field: 'top_p', verdict: { verdict: 'differs', expected: 0.95, observed: 0.9 } },
                  {
                    field: 'min_p',
                    verdict: { verdict: 'indeterminate', reason: 'not reported by this build' },
                  },
                ],
              },
            },
          })}
        />,
      );

      expect(screen.getByText(/defaults have moved/i)).toBeInTheDocument();
    });

    // Several distinct causes exist now; showing only the first hid whichever
    // did not happen to sort first.
    it('lists every distinct reason a field could not be checked', () => {
      render(
        <ProxySamplingPanel
          audit={audit({
            baseline: {
              state: 'read',
              report: {
                coverage: { coverage: 'blind', indeterminate: 2 },
                fields: [
                  {
                    field: 'top_p',
                    verdict: { verdict: 'indeterminate', reason: 'not reported by this build' },
                  },
                  {
                    field: 'min_p',
                    verdict: { verdict: 'indeterminate', reason: 'something else entirely' },
                  },
                ],
              },
            },
          })}
        />,
      );

      expect(screen.getByText(/not reported by this build/i)).toBeInTheDocument();
      expect(screen.getByText(/something else entirely/i)).toBeInTheDocument();
    });

    // The baseline half's version of the rule the whole panel obeys. A read
    // that was attempted and failed is not a read that has not happened, and
    // "not read yet" is a claim about the poller rather than about the server.
    it('renders an unreadable baseline as a warning carrying its cause', () => {
      render(
        <ProxySamplingPanel
          audit={audit({
            baseline: {
              state: 'unreadable',
              reason: '/props is unreadable: connection refused.',
            },
          })}
        />,
      );

      expect(screen.getByText(/could not be read/i)).toBeInTheDocument();
      expect(screen.getByText(/connection refused/i)).toBeInTheDocument();
      expect(screen.queryByText(/not read yet/i)).not.toBeInTheDocument();
      expect(screen.queryByText(/match the values/i)).not.toBeInTheDocument();
    });

    it('says so plainly when no read has been attempted yet', () => {
      render(<ProxySamplingPanel audit={audit({ baseline: { state: 'not_yet_read' } })} />);

      expect(screen.getByText(/not read yet/i)).toBeInTheDocument();
      expect(screen.queryByText(/could not be read/i)).not.toBeInTheDocument();
    });
  });

  // Discarding client values is the default configuration working, not a
  // fault — but it is the answer to "why is my temperature ignored", so it has
  // to be visible and it has to say why.
  it('explains discarded client values rather than presenting them as errors', () => {
    render(<ProxySamplingPanel audit={audit({ client_fields_discarded: 88 })} />);
    expect(screen.getByText(/88 \(trust_client_sampling is off\)/)).toBeInTheDocument();
  });

  it('degrades to a plain note on a proxy that does not report the readback', () => {
    render(<ProxySamplingPanel audit={null} />);
    expect(screen.getByText(/does not report the sampling readback/i)).toBeInTheDocument();
  });

  // `0.05f32` widened to f64 is 0.05000000074505806. Printing that verbatim
  // would make every correctly-transmitted value look like a defect.
  it('trims f32-through-JSON noise out of displayed values', () => {
    expect(formatValue(0.05000000074505806)).toBe('0.05');
    expect(formatValue(0.949999988079071)).toBe('0.95');
    expect(formatValue(1.5)).toBe('1.5');
    expect(formatValue(40)).toBe('40');
  });
});
