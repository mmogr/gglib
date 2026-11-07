/**
 * Tests for the loop guard's log panel — the GUI face of `gglib proxy trips`
 * — given the state `useLoopGuardTrips` holds, as the Live tab hands it.
 *
 * The load-bearing behaviours: a day the guard scanned without a trip is
 * shown with zero trips; a failed read offers a Retry, and Retry's read in
 * flight says it is reading; Refresh reads again. Neither button submits a
 * form around it. The read itself, and when Live draws the panel, are
 * `places/diagnostics.test.tsx`'s.
 */

import { describe, it, expect, vi } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import { LoopGuardTripsPanel } from '../../../src/places/diagnostics/live/LoopGuardTripsPanel';
import type { LoopGuardTripsState } from '../../../src/places/diagnostics/live/useLoopGuardTrips';
import type { LoopGuardTripDay } from '../../../src/types/generated/LoopGuardTripDay';

const day = (overrides: Partial<LoopGuardTripDay> = {}): LoopGuardTripDay => ({
  day: '2026-09-18',
  model_name: 'qwen3-coder',
  gglib_version: '0.18.0',
  mode: 'note',
  scanned: 40,
  trips: 3,
  loops: 2,
  stagnations: 1,
  sessions: 2,
  ...overrides,
});

const state = (overrides: Partial<LoopGuardTripsState> = {}): LoopGuardTripsState => ({
  days: null,
  loading: false,
  error: null,
  reload: vi.fn(async () => {}),
  ...overrides,
});

describe('LoopGuardTripsPanel', () => {
  it('shows each day, a day scanned without a trip included', () => {
    render(
      <LoopGuardTripsPanel
        state={state({ days: [day(), day({ day: '2026-09-17', trips: 0, loops: 0, stagnations: 0, sessions: 0, scanned: 12 })] })}
      />,
    );

    const rows = screen.getAllByRole('row');
    // A header row and one row per day.
    expect(rows).toHaveLength(3);
    const quiet = within(rows[2]).getAllByRole('cell').map((c) => c.textContent);
    expect(quiet).toEqual(['2026-09-17', 'qwen3-coder', '0.18.0', 'note', '12', '0', '0', '0', '0']);
    const busy = within(rows[1]).getAllByRole('cell').map((c) => c.textContent);
    expect(busy).toEqual(['2026-09-18', 'qwen3-coder', '0.18.0', 'note', '40', '3', '2', '1', '2']);
    expect(screen.getByRole('heading', { name: 'What it did, the last 30 days' })).toBeInTheDocument();
  });

  it('offers a Retry on a failed read that reads again and submits no form around it', async () => {
    const onSubmit = vi.fn((event: { preventDefault: () => void }) => event.preventDefault());
    const failed = state({ error: 'daemon unreachable' });
    render(
      <form onSubmit={onSubmit}>
        <LoopGuardTripsPanel state={failed} />
      </form>,
    );

    expect(screen.getByText('daemon unreachable')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Retry' }));
    expect(failed.reload).toHaveBeenCalledTimes(1);
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("says it is reading while Retry's read is in flight", () => {
    render(<LoopGuardTripsPanel state={state({ error: 'daemon unreachable', loading: true })} />);
    expect(screen.getByText("Reading the loop guard's log…")).toBeInTheDocument();
    expect(screen.queryByText('daemon unreachable')).not.toBeInTheDocument();
  });

  it('Refresh reads again and submits no form around it', async () => {
    const onSubmit = vi.fn((event: { preventDefault: () => void }) => event.preventDefault());
    const shown = state({ days: [day()] });
    render(
      <form onSubmit={onSubmit}>
        <LoopGuardTripsPanel state={shown} />
      </form>,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Refresh' }));
    expect(shown.reload).toHaveBeenCalledTimes(1);
    expect(onSubmit).not.toHaveBeenCalled();
  });
});
