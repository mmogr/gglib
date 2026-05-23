import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import WaveScrubber from './WaveScrubber';
import type { OrchestratorRunEvent } from '../../../types/orchestrator';

// ─── Helpers ─────────────────────────────────────────────────────────────────

function makeEvent(
  seq: number,
  waveIndex: number,
  type: string,
  extra: Record<string, unknown> = {},
): OrchestratorRunEvent {
  return {
    run_id: 'run-1',
    seq,
    event_json: JSON.stringify({ type, wave_index: waveIndex, ...extra }),
    created_at: `2026-01-01T00:00:0${seq}Z`,
    wave_index: waveIndex,
  };
}

function makeWaveEvents(waveCount: number): OrchestratorRunEvent[] {
  const events: OrchestratorRunEvent[] = [];
  let seq = 0;
  for (let w = 0; w < waveCount; w++) {
    events.push(makeEvent(seq++, w, 'node_complete', { node_id: `n${w}` }));
    events.push(makeEvent(seq++, w, 'wave_completed', { node_count: 1 }));
  }
  return events;
}

// ─── Tests ───────────────────────────────────────────────────────────────────

describe('WaveScrubber', () => {
  it('renders nothing when there are fewer than 2 wave boundaries', () => {
    const { container } = render(
      <WaveScrubber events={makeWaveEvents(1)} onRewind={vi.fn()} />,
    );
    expect(container.firstChild).toBeNull();
  });

  it('renders nothing for empty event list', () => {
    const { container } = render(<WaveScrubber events={[]} onRewind={vi.fn()} />);
    expect(container.firstChild).toBeNull();
  });

  it('renders wave scrubber section with 2+ waves', () => {
    render(<WaveScrubber events={makeWaveEvents(3)} onRewind={vi.fn()} />);
    expect(screen.getByTestId('wave-scrubber')).toBeInTheDocument();
  });

  it('renders a badge for each completed wave', () => {
    render(<WaveScrubber events={makeWaveEvents(3)} onRewind={vi.fn()} />);
    // Each wave badge shows "W0", "W1", "W2"
    expect(screen.getByText(/W0/)).toBeInTheDocument();
    expect(screen.getByText(/W1/)).toBeInTheDocument();
    expect(screen.getByText(/W2/)).toBeInTheDocument();
  });

  it('shows confirmation dialog when a wave badge is clicked', () => {
    render(<WaveScrubber events={makeWaveEvents(3)} onRewind={vi.fn()} />);
    fireEvent.click(screen.getByText(/W0/));
    expect(screen.getByRole('alertdialog')).toBeInTheDocument();
    expect(screen.getByTestId('rewind-confirm-btn')).toBeInTheDocument();
  });

  it('calls onRewind with the correct wave index on confirm', () => {
    const onRewind = vi.fn();
    render(<WaveScrubber events={makeWaveEvents(3)} onRewind={onRewind} />);
    fireEvent.click(screen.getByText(/W0/));
    fireEvent.click(screen.getByTestId('rewind-confirm-btn'));
    expect(onRewind).toHaveBeenCalledWith(0);
  });

  it('dismisses dialog on cancel without calling onRewind', () => {
    const onRewind = vi.fn();
    render(<WaveScrubber events={makeWaveEvents(3)} onRewind={onRewind} />);
    fireEvent.click(screen.getByText(/W1/));
    expect(screen.getByRole('alertdialog')).toBeInTheDocument();

    const cancelBtn = screen.getByRole('button', { name: /cancel/i });
    fireEvent.click(cancelBtn);

    expect(onRewind).not.toHaveBeenCalled();
    expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument();
  });

  it('does not open dialog when disabled', () => {
    render(<WaveScrubber events={makeWaveEvents(3)} onRewind={vi.fn()} disabled />);
    fireEvent.click(screen.getByText(/W0/));
    expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument();
  });
});
