/**
 * The usage donut: the slot card's, with its figure in the middle, and the
 * compact ring the chat composer draws, which is the ring alone.
 */

import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import { ContextUsageDonut } from '../../../src/components/ContextUsageDonut';

/** The arc that shows the usage: the second circle, over the track. */
const arc = (container: HTMLElement) => container.querySelectorAll('circle')[1];

describe('ContextUsageDonut', () => {
  it('draws its figure and its caption in the middle', () => {
    const { container } = render(<ContextUsageDonut used={420} total={1000} label="Slot 0" />);
    expect(screen.getByText('42%')).toBeInTheDocument();
    expect(screen.getByText('Slot 0')).toBeInTheDocument();
    expect(container.querySelector('svg')).not.toHaveAttribute('aria-hidden');
  });

  it('draws a dash, not a figure, when the usage is not known', () => {
    const { container } = render(<ContextUsageDonut used={null} total={4096} />);
    expect(screen.getByText('—')).toBeInTheDocument();
    expect(screen.queryByText(/%/)).not.toBeInTheDocument();
    expect(arc(container)).toHaveClass('stroke-primary');
  });

  it('compact, draws the ring alone: no figure, no dash, no caption, and hidden from assistive tech', () => {
    const { container } = render(<ContextUsageDonut compact used={420} total={1000} label="Slot 0" size={18} strokeWidth={3} />);
    expect(container.textContent).toBe('');
    expect(container.querySelectorAll('circle')).toHaveLength(2);
    expect(container.querySelector('svg')).toHaveAttribute('aria-hidden', 'true');

    const unknown = render(<ContextUsageDonut compact used={null} total={null} />);
    expect(unknown.container.textContent).toBe('');
  });

  it('wears the accent under 70, the warning from 70 and danger from 90', () => {
    const stroke = (used: number, total: number) => arc(render(<ContextUsageDonut used={used} total={total} />).container);
    expect(stroke(1388, 2000)).toHaveClass('stroke-primary');
    expect(stroke(1400, 2000)).toHaveClass('stroke-warning');
    expect(stroke(1788, 2000)).toHaveClass('stroke-warning');
    expect(stroke(1800, 2000)).toHaveClass('stroke-danger');
  });

  it('colours and words an exact half the same way: 139 of 200 is 70% and a warning', () => {
    const { container } = render(<ContextUsageDonut used={139} total={200} />);
    expect(screen.getByText('70%')).toBeInTheDocument();
    expect(arc(container)).toHaveClass('stroke-warning');
  });

  it('says the whole-number percent every meter says: 113 of 200 is 57%', () => {
    render(<ContextUsageDonut used={113} total={200} />);
    expect(screen.getByText('57%')).toBeInTheDocument();
  });

  it('never draws a figure over 100 or under 0', () => {
    render(<ContextUsageDonut used={5000} total={4096} />);
    expect(screen.getByText('100%')).toBeInTheDocument();
    render(<ContextUsageDonut used={-100} total={4096} />);
    expect(screen.getByText('0%')).toBeInTheDocument();
  });
});
