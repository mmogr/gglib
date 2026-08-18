import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import ProxyReasoningRows, {
  DroppedClientFields,
} from '../../../src/components/ProxyReasoningRows';
import type {
  SamplingEffortSupport,
  SamplingReasoningReadback,
} from '../../../src/services/transport/types/dashboard';
import { reasoningReadback } from '../fixtures/dashboard';

const reasoning = (
  overrides: Partial<SamplingReasoningReadback> = {},
): SamplingReasoningReadback => reasoningReadback(overrides);

describe('ProxyReasoningRows', () => {
  // The collapse ADR 0007 decision 3 forbids, at the last layer that can make
  // it: "the template ignores this" and "nobody has asked" license opposite
  // conclusions, and only the first says an effort setting is inert.
  it('renders an unobserved template as unknown, never as unsupported', () => {
    render(<ProxyReasoningRows reasoning={reasoning()} />);

    expect(screen.getByText(/support for reasoning_effort is unknown/i)).toBeInTheDocument();
    // The reason string the proxy actually sends — `EffortSupportState::of`
    // in `audit_records.rs`. The old assertion matched a shortened version
    // that came from the fixture rather than from the wire.
    expect(
      screen.getByText(/no \/props read has completed for the running model yet/),
    ).toBeInTheDocument();
    expect(screen.queryByText(/never reads reasoning_effort/i)).not.toBeInTheDocument();
  });

  // A warning, not a tick: the reason is what makes the state readable, and a
  // neutral "not read yet" would be a claim about a read that did happen.
  it('carries the cause of an unobserved template as a warning', () => {
    render(
      <ProxyReasoningRows
        reasoning={reasoning({
          effort_support: {
            state: 'not_yet_observed',
            reason: 'this server’s template capabilities could not be read: refused',
          },
        })}
      />,
    );

    expect(screen.getByRole('status')).toHaveTextContent(/could not be read: refused/);
  });

  it('says plainly when the template positively does not read the variable', () => {
    render(
      <ProxyReasoningRows reasoning={reasoning({ effort_support: { state: 'not_supported' } })} />,
    );

    expect(screen.getByText(/never reads reasoning_effort/i)).toBeInTheDocument();
    expect(screen.queryByText(/is unknown/i)).not.toBeInTheDocument();
  });

  // Version skew is the documented posture of this component, not a hypothesis:
  // it is a real HTTP client of a separately deployed proxy's contract, so a
  // newer proxy can name a state this build's union does not have. Rendering it
  // as "yes" would let skew alone report health; rendering it as `not_supported`
  // would declare every effort setting on the model inert. The cast is the point
  // of the test — the value is legal on the wire and illegal in the type.
  it('reads a state this build does not recognise as unknown, never as yes', () => {
    render(
      <ProxyReasoningRows
        reasoning={reasoning({
          effort_support: {
            state: 'supported_but_ignored_on_tuesdays',
          } as unknown as SamplingEffortSupport,
        })}
      />,
    );

    expect(screen.getByText(/support for reasoning_effort is unknown/i)).toBeInTheDocument();
    expect(screen.getByText(/does not recognise/i)).toBeInTheDocument();
    expect(screen.queryByText('Template reads reasoning_effort')).not.toBeInTheDocument();
    expect(screen.queryByText('yes')).not.toBeInTheDocument();
    expect(screen.queryByText(/never reads reasoning_effort/i)).not.toBeInTheDocument();
  });

  it('shows a supported template as a plain row rather than an alarm', () => {
    render(
      <ProxyReasoningRows reasoning={reasoning({ effort_support: { state: 'supported' } })} />,
    );

    expect(screen.getByText('Template reads reasoning_effort')).toBeInTheDocument();
    expect(screen.getByText('yes')).toBeInTheDocument();
  });

  // The record exists precisely because nothing else in the system holds it:
  // the level is deleted from the body before sending, and no readback can
  // ever report that it was.
  it('shows a suppressed level with its rung and marks it as never sent', () => {
    render(
      <ProxyReasoningRows
        reasoning={reasoning({
          effort_support: { state: 'not_supported' },
          latest: { effort: { level: 'high', source: 'profile', suppressed: true }, budget: null },
        })}
      />,
    );

    expect(screen.getByText(/was never sent/i)).toBeInTheDocument();
    expect(screen.getByText(/high, from profile/)).toBeInTheDocument();
  });

  // The panel's own rule, applied to a blindness that is permanent rather than
  // a fault: a reader who takes these rows for a readback has been told
  // something this system cannot know.
  it('prints the blindness beside a value it qualifies, never as health', () => {
    render(
      <ProxyReasoningRows
        reasoning={reasoning({
          effort_support: { state: 'supported' },
          latest: { effort: { level: 'medium', source: 'global', suppressed: false }, budget: null },
        })}
      />,
    );

    expect(screen.getByText(/neither reasoning control can be read back/i)).toBeInTheDocument();
    expect(screen.getByText(/task_params::to_json/)).toBeInTheDocument();
  });

  it('omits the blindness note when there is no value to qualify', () => {
    render(
      <ProxyReasoningRows
        reasoning={reasoning({
          effort_support: { state: 'supported' },
          latest: { effort: null, budget: null },
        })}
      />,
    );

    expect(screen.getByText(/named neither reasoning control/i)).toBeInTheDocument();
    expect(screen.queryByText(/neither reasoning control can be read back/i)).not.toBeInTheDocument();
  });

  // Two different facts: no request has been resolved, versus a request that
  // named neither control.
  it('separates "nothing resolved yet" from "resolved and named neither"', () => {
    render(<ProxyReasoningRows reasoning={reasoning({ effort_support: { state: 'supported' } })} />);

    expect(screen.getByText(/no request has resolved either/i)).toBeInTheDocument();
  });

  it('renders nothing at all on a proxy that predates the field', () => {
    const { container } = render(<ProxyReasoningRows reasoning={null} />);
    expect(container).toBeEmptyDOMElement();
  });
});

describe('DroppedClientFields', () => {
  // The count said four fields were dropped; only the name answers "is gglib
  // ignoring the reasoning_effort I sent?".
  it('names each dropped field and keeps the two kinds of drop apart', () => {
    render(
      <DroppedClientFields
        names={{
          fields: [
            { field: 'reasoning_effort', discarded: 12, rejected: 0 },
            { field: 'top_k', discarded: 0, rejected: 2 },
          ],
          untracked: 0,
        }}
      />,
    );

    expect(screen.getByText('reasoning_effort')).toBeInTheDocument();
    expect(screen.getByText(/12 dropped as untrusted/)).toBeInTheDocument();
    expect(screen.getByText(/2 unreadable as sent/)).toBeInTheDocument();
  });

  // A bound nobody can see is indistinguishable from a bound nobody hit.
  it('says when drops went untracked past the tally bound', () => {
    render(
      <DroppedClientFields
        names={{ fields: [{ field: 'temperature', discarded: 1, rejected: 0 }], untracked: 7 }}
      />,
    );

    expect(screen.getByText(/7 further drops were not tracked/)).toBeInTheDocument();
  });

  it('renders nothing when no client field has been dropped', () => {
    const { container } = render(<DroppedClientFields names={{ fields: [], untracked: 0 }} />);
    expect(container).toBeEmptyDOMElement();
  });
});
