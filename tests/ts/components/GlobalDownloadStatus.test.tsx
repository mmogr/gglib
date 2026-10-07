/**
 * The download card prints the row the daemon sent and works nothing out.
 *
 * The CLI draws the same row, so a download reads the same in both only while
 * neither makes its own words. Each fixture here pairs numbers with text that
 * does not follow from them: a card that formatted the numbers itself would
 * print something else and fail.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import '@testing-library/jest-dom';
import { GlobalDownloadStatus } from '../../../src/components/GlobalDownloadStatus';
import type { QueueSnapshot } from '../../../src/services/transport/types/downloads';
import { queueSnapshot, runningRow, waitingRow } from '../fixtures/downloads';

const transport = vi.hoisted(() => ({ removeFromQueue: vi.fn(), reorderQueueItem: vi.fn() }));

vi.mock('../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../src/services/transport')>()),
  getTransport: () => transport,
}));

function renderCard(snapshot: QueueSnapshot | null, props: { cancellingId?: string | null; onCancel?: (id: string) => void } = {}) {
  return render(
    <GlobalDownloadStatus
      snapshot={snapshot}
      cancellingId={props.cancellingId ?? null}
      lastQueueSummary={null}
      onCancel={props.onCancel ?? (() => {})}
      onDismissSummary={() => {}}
    />,
  );
}

/** The bar's fill, the one element whose width is set inline. */
const fill = (container: HTMLElement) => container.querySelector<HTMLElement>('.bg-primary.h-full')!;

describe('GlobalDownloadStatus', () => {
  beforeEach(() => {
    transport.removeFromQueue.mockReset().mockResolvedValue(undefined);
    transport.reorderQueueItem.mockReset().mockResolvedValue(undefined);
  });

  it('renders the row text verbatim', () => {
    // Numbers and words that disagree on purpose, and a title too long for
    // the 50 characters the card once cut it to.
    const title = 'owner/a-repository-with-a-very-long-name-indeed-GGUF:Q8_0';
    const row = runningRow({
      downloaded_bytes: 1,
      total_bytes: 2,
      percent: 40,
      speed_bps: 5,
      eta_seconds: 9,
      text: {
        title,
        file: 'part 2/3',
        status: 'preparing fast downloader…',
        bytes: '7.00 GiB / 28.00 GiB',
        percent: '25.0%',
        speed: '118.4 MB/s',
        eta: 'ETA 2m 40s',
      },
    });

    const { container } = renderCard(queueSnapshot({ active: row }));

    for (const words of [title, 'part 2/3', 'preparing fast downloader…', '7.00 GiB / 28.00 GiB', '25.0%', '118.4 MB/s', 'ETA 2m 40s']) {
      expect(screen.getByText(words)).toBeInTheDocument();
    }
    // The bar is the row's own percentage, not one worked out from its bytes.
    expect(fill(container).style.width).toBe('40%');
    // Nothing of the card's own making: no phase table, no second bar.
    expect(screen.queryByText(/shard/i)).not.toBeInTheDocument();
    expect(screen.queryByText('Downloading')).not.toBeInTheDocument();
    expect(container.querySelectorAll('.bg-primary.h-full')).toHaveLength(1);
  });

  it('keeps the card between files', () => {
    // Between two files nothing is being fetched. The daemon still sends the
    // row as `active` and `downloading`: it names the file that is next, and
    // keeps the speed and the time remaining its meter last read.
    const row = runningRow();
    const gap = { ...row, text: { ...row.text, file: 'projector' } };

    const { container } = renderCard(queueSnapshot({ active: gap }));

    expect(screen.getByText(gap.text.title)).toBeInTheDocument();
    expect(screen.getByText('projector')).toBeInTheDocument();
    expect(screen.getByText('7.00 GiB / 28.00 GiB')).toBeInTheDocument();
    expect(screen.getByText('118.4 MB/s')).toBeInTheDocument();
    expect(fill(container).style.width).toBe('25%');
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeEnabled();
  });

  it('keeps the card while the model is finalized and registered', () => {
    // Every byte is in. The row is still `active`, in a later phase, with no
    // speed and no time remaining, as `row.rs` makes it.
    const { speed_bps: _speed, eta_seconds: _eta, ...row } = runningRow();
    const finalizing = {
      ...row,
      phase: 'finalizing' as const,
      downloaded_bytes: row.total_bytes!,
      percent: 100,
      text: { ...row.text, status: 'Finalizing…', bytes: '28.00 GiB / 28.00 GiB', percent: '100.0%', speed: '', eta: '' },
    };

    const { container } = renderCard(queueSnapshot({ active: finalizing }));

    expect(screen.getByText('Finalizing…')).toBeInTheDocument();
    expect(screen.getByText(finalizing.text.title)).toBeInTheDocument();
    expect(screen.getByText('100.0%')).toBeInTheDocument();
    expect(fill(container).style.width).toBe('100%');
    expect(screen.queryByText(/ETA/)).not.toBeInTheDocument();
  });

  it('draws a download of unknown size as a bar with no fill width and no percentage', () => {
    const { percent: _percent, total_bytes: _total, ...row } = runningRow();
    const unsized = { ...row, text: { ...row.text, bytes: '7.00 GiB', percent: '' } };

    const { container } = renderCard(queueSnapshot({ active: unsized }));

    expect(fill(container).style.width).toBe('');
    expect(fill(container)).toHaveClass('animate-indeterminate');
    expect(screen.getByText('7.00 GiB')).toBeInTheDocument();
    expect(screen.queryByText(/%/)).not.toBeInTheDocument();
  });

  it('chip counts waiting downloads', async () => {
    // Three files wait behind the running one, in two downloads. The running
    // download's own next files are not rows at all.
    const waiting = [
      waitingRow('owner/b:Q4_K_M', 2, { text: { ...waitingRow('owner/b:Q4_K_M', 2).text, file: '3 parts' } }),
      waitingRow('owner/c:Q4_K_M', 3),
    ];
    renderCard(queueSnapshot({ active: runningRow(), waiting }));

    const chip = screen.getByText('+2 queued');
    await userEvent.setup().click(chip);

    // The popover lists the same two, each under its own title, and prints
    // the row's "3 parts" where it has one.
    expect(screen.getByText('2 items')).toBeInTheDocument();
    expect(screen.getByText('owner/b:Q4_K_M')).toBeInTheDocument();
    expect(screen.getByText('owner/c:Q4_K_M')).toBeInTheDocument();
    expect(screen.getAllByText(/parts/)).toHaveLength(1);
    expect(screen.getByText('3 parts')).toBeInTheDocument();
  });

  it("reorders by the row's position and removes by the row's id", async () => {
    // Positions count from the running download, so the first waiting row is
    // at 2: its place in the list and its position are different numbers.
    const waiting = [waitingRow('owner/b:Q4_K_M', 2), waitingRow('owner/c:Q4_K_M', 3)];
    const onRefreshQueue = vi.fn();
    render(
      <GlobalDownloadStatus
        snapshot={queueSnapshot({ active: runningRow(), waiting })}
        cancellingId={null}
        lastQueueSummary={null}
        onCancel={() => {}}
        onDismissSummary={() => {}}
        onRefreshQueue={onRefreshQueue}
      />,
    );
    const user = userEvent.setup();
    await user.click(screen.getByText('+2 queued'));

    await user.click(screen.getAllByRole('button', { name: 'Move down in queue' })[0]);
    expect(transport.reorderQueueItem).toHaveBeenLastCalledWith('owner/b:Q4_K_M', 3);

    await user.click(screen.getAllByRole('button', { name: 'Move up in queue' })[1]);
    expect(transport.reorderQueueItem).toHaveBeenLastCalledWith('owner/c:Q4_K_M', 2);
    expect(transport.reorderQueueItem).toHaveBeenCalledTimes(2);

    await user.click(screen.getAllByRole('button', { name: 'Remove from queue' })[0]);
    expect(transport.removeFromQueue).toHaveBeenCalledWith('owner/b:Q4_K_M');
    expect(onRefreshQueue).toHaveBeenCalledTimes(3);
  });

  it('shows no chip when nothing waits, and nothing at all when nothing runs', () => {
    const { unmount } = renderCard(queueSnapshot({ active: runningRow() }));
    expect(screen.queryByText(/queued/)).not.toBeInTheDocument();
    unmount();

    // Waiting downloads alone do not draw the card: it is the running row.
    const { container } = renderCard(queueSnapshot({ waiting: [waitingRow('owner/b', 1)] }));
    expect(container).toBeEmptyDOMElement();
    expect(renderCard(null).container).toBeEmptyDOMElement();
  });

  it('cancels the running download by its id, and waits while a cancel is out', async () => {
    const onCancel = vi.fn();
    const row = runningRow();
    const { unmount } = renderCard(queueSnapshot({ active: row }), { onCancel });

    await userEvent.setup().click(screen.getByRole('button', { name: 'Cancel' }));
    expect(onCancel).toHaveBeenCalledWith(row.id);
    unmount();

    const card = renderCard(queueSnapshot({ active: row }), { cancellingId: row.id });
    expect(within(card.container).getByRole('button', { name: 'Cancelling...' })).toBeDisabled();
  });
});
