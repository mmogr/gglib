/**
 * Tests for the Readout primitive.
 *
 * The single treatment for live metrics: mono tabular value, quiet unit,
 * intent coloring the value only, and the trend slot.
 */

import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { Readout } from '../../../src/components/primitives/Readout';

describe('Readout', () => {
  it('renders label, value, and unit', () => {
    render(<Readout label="Generation" value="127.4" unit="tok/s" />);

    expect(screen.getByText('Generation')).toBeInTheDocument();
    expect(screen.getByText('127.4')).toBeInTheDocument();
    expect(screen.getByText('tok/s')).toBeInTheDocument();
  });

  it('renders the value in mono with tabular figures', () => {
    render(<Readout label="KV cache" value={62} unit="%" />);

    const value = screen.getByText('62');
    expect(value.className).toContain('font-mono');
    expect(value.className).toContain('tabular-nums');
  });

  it('colors only the value with the intent, never the label or unit', () => {
    render(<Readout label="KV cache" value={92} unit="%" intent="danger" />);

    expect(screen.getByText('92').className).toContain('text-danger');
    expect(screen.getByText('KV cache').className).toContain('text-text-muted');
    expect(screen.getByText('%').className).toContain('text-text-muted');
  });

  it('renders the trend slot under the value', () => {
    render(
      <Readout
        label="Generation"
        value="127.4"
        unit="tok/s"
        trend={<svg data-testid="trend-mark" />}
      />,
    );

    expect(screen.getByTestId('trend-mark')).toBeInTheDocument();
  });
});
