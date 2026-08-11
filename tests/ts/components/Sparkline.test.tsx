/**
 * Tests for the Sparkline primitive.
 *
 * The app's single micro-chart style: accessible-name requirement, the
 * under-two-samples baseline placeholder, currentColor marks, and the
 * fixed-domain option.
 */

import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { Sparkline } from '../../../src/components/primitives/Sparkline';

describe('Sparkline', () => {
  it('exposes an accessible name as an image', () => {
    render(<Sparkline values={[1, 2, 3]} ariaLabel="Generation rate, last 60 samples" />);

    expect(
      screen.getByRole('img', { name: 'Generation rate, last 60 samples' }),
    ).toBeInTheDocument();
  });

  it('renders only the baseline until two samples exist', () => {
    const { container } = render(<Sparkline values={[42]} ariaLabel="Warming up" />);

    expect(container.querySelector('line')).not.toBeNull();
    expect(container.querySelector('polyline')).toBeNull();
    expect(container.querySelector('circle')).toBeNull();
  });

  it('renders the line and a terminus dot in currentColor once data exists', () => {
    const { container } = render(<Sparkline values={[1, 5, 3]} ariaLabel="Series" />);

    const polyline = container.querySelector('polyline');
    const dot = container.querySelector('circle');
    expect(polyline?.getAttribute('stroke')).toBe('currentColor');
    expect(dot?.getAttribute('fill')).toBe('currentColor');
  });

  it('places the terminus dot on the last sample', () => {
    const { container } = render(
      <Sparkline values={[0, 10]} width={64} height={20} ariaLabel="Series" />,
    );

    const polyline = container.querySelector('polyline');
    const dot = container.querySelector('circle');
    const lastPoint = polyline?.getAttribute('points')?.split(' ').at(-1);
    const [lastX, lastY] = (lastPoint ?? '').split(',').map(Number);
    expect(lastX).toBeCloseTo(Number(dot?.getAttribute('cx')), 2);
    expect(lastY).toBeCloseTo(Number(dot?.getAttribute('cy')), 2);
  });

  it('respects a fixed domain instead of autoscaling', () => {
    const { container } = render(
      <Sparkline values={[50, 50]} min={0} max={100} height={20} ariaLabel="Percent" />,
    );

    const points = container.querySelector('polyline')?.getAttribute('points')?.split(' ');
    const y = Number(points?.[0]?.split(',')[1]);
    expect(y).toBeCloseTo(10, 1);
  });
});
