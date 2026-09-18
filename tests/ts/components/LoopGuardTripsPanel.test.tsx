/**
 * Tests for the loop guard's log panel — the GUI face of `gglib proxy trips`.
 *
 * The load-bearing behaviours: a day the guard scanned without a trip is
 * shown with zero trips, an empty log says so, and a failed read offers a
 * Retry that does not submit the settings form the panel sits in.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import { LoopGuardTripsPanel } from '../../../src/components/SettingsModal/LoopGuardTripsPanel';
import { getLoopGuardTrips } from '../../../src/services/transport/api/proxy';
import type { LoopGuardTripDay } from '../../../src/types/generated/LoopGuardTripDay';

vi.mock('../../../src/services/transport/api/proxy', () => ({
  getLoopGuardTrips: vi.fn(),
}));

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

describe('LoopGuardTripsPanel', () => {
  beforeEach(() => {
    vi.mocked(getLoopGuardTrips).mockReset();
  });

  it('shows each day, a day scanned without a trip included', async () => {
    vi.mocked(getLoopGuardTrips).mockResolvedValue([
      day(),
      day({ day: '2026-09-17', trips: 0, loops: 0, stagnations: 0, sessions: 0, scanned: 12 }),
    ]);

    render(<LoopGuardTripsPanel />);

    const rows = await screen.findAllByRole('row');
    // A header row and one row per day.
    expect(rows).toHaveLength(3);
    const quiet = within(rows[2]).getAllByRole('cell').map((c) => c.textContent);
    expect(quiet).toEqual(['2026-09-17', 'qwen3-coder', '0.18.0', 'note', '12', '0', '0', '0', '0']);
    const busy = within(rows[1]).getAllByRole('cell').map((c) => c.textContent);
    expect(busy).toEqual(['2026-09-18', 'qwen3-coder', '0.18.0', 'note', '40', '3', '2', '1', '2']);
    // The window it names is the window it asked for.
    expect(getLoopGuardTrips).toHaveBeenCalledWith(30);
  });

  it('says so when the log holds nothing', async () => {
    vi.mocked(getLoopGuardTrips).mockResolvedValue([]);

    render(<LoopGuardTripsPanel />);

    expect(await screen.findByText(/Nothing scanned in the last 30 days/)).toBeInTheDocument();
    expect(screen.queryByRole('table')).not.toBeInTheDocument();
  });

  it('offers a Retry that reads again and does not submit the form it sits in', async () => {
    vi.mocked(getLoopGuardTrips)
      .mockRejectedValueOnce(new Error('daemon unreachable'))
      .mockResolvedValueOnce([day()]);
    const onSubmit = vi.fn((event: { preventDefault: () => void }) => event.preventDefault());

    render(
      <form onSubmit={onSubmit}>
        <LoopGuardTripsPanel />
      </form>,
    );

    expect(await screen.findByText('daemon unreachable')).toBeInTheDocument();
    await userEvent.click(screen.getByRole('button', { name: 'Retry' }));

    await waitFor(() => expect(screen.getAllByRole('row')).toHaveLength(2));
    expect(getLoopGuardTrips).toHaveBeenCalledTimes(2);

    // Refresh sits in the same form, and must not submit it either.
    vi.mocked(getLoopGuardTrips).mockResolvedValueOnce([day()]);
    await userEvent.click(screen.getByRole('button', { name: 'Refresh' }));
    await waitFor(() => expect(getLoopGuardTrips).toHaveBeenCalledTimes(3));
    await waitFor(() => expect(screen.getAllByRole('row')).toHaveLength(2));
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it('shows that it is reading until the log arrives', () => {
    vi.mocked(getLoopGuardTrips).mockReturnValue(new Promise(() => {}));

    render(<LoopGuardTripsPanel />);

    expect(screen.getByText(/Reading the loop guard/)).toBeInTheDocument();
  });
});
