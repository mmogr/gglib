/**
 * The download queue the GUI holds is the daemon's snapshot, taken whole.
 *
 * It arrives two ways, from `GET /api/models/downloads/queue` and in
 * `queue_snapshot` events, numbered in one sequence. What is pinned here is
 * which snapshot wins: the one with the higher `revision`, until the event
 * stream opens again, when the numbering may have started over.
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { useDownloadManager } from '../../../src/hooks/useDownloadManager';
import type { DownloadEvent } from '../../../src/services/transport/types/events';
import type { QueueSnapshot } from '../../../src/services/transport/types/downloads';
import { queueSnapshot, runningRow, waitingRow } from '../fixtures/downloads';

// The transport: the queue routes, the event subscription and the signal
// that the stream opened. The test plays the daemon through the two handlers.
let sendEvent: ((event: { type: 'download'; event: DownloadEvent }) => void) | null = null;
let streamOpened: (() => void) | null = null;
let unsubscribed: string[] = [];

const transport = vi.hoisted(() => ({
  queueDownload: vi.fn(),
  getDownloadQueue: vi.fn(),
  cancelDownload: vi.fn(),
  subscribe: vi.fn(),
  onEventStreamOpen: vi.fn(),
}));

vi.mock('../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../src/services/transport')>()),
  getTransport: () => transport,
}));

/** Deliver a download event as the stream does. */
function emit(event: DownloadEvent) {
  act(() => sendEvent?.({ type: 'download', event }));
}

/** A `queue_snapshot` event carrying `snapshot`. */
const snapshotEvent = (snapshot: QueueSnapshot): DownloadEvent => ({ type: 'queue_snapshot', ...snapshot });

/** A REST answer the test settles when it chooses. */
function heldAnswer() {
  let settle!: (snapshot: QueueSnapshot) => void;
  transport.getDownloadQueue.mockImplementationOnce(() => new Promise((resolve) => { settle = resolve; }));
  return (snapshot: QueueSnapshot) => act(async () => settle(snapshot));
}

describe('useDownloadManager', () => {
  beforeEach(() => {
    sendEvent = null;
    streamOpened = null;
    unsubscribed = [];
    for (const mock of Object.values(transport)) mock.mockReset();
    transport.getDownloadQueue.mockResolvedValue(queueSnapshot());
    transport.subscribe.mockImplementation((_type: string, handler: typeof sendEvent) => {
      sendEvent = handler;
      return () => unsubscribed.push('events');
    });
    transport.onEventStreamOpen.mockImplementation((handler: () => void) => {
      streamOpened = handler;
      return () => unsubscribed.push('open');
    });
  });

  it('seeds from REST on mount', async () => {
    const seeded = queueSnapshot({ revision: 7, active: runningRow(), waiting: [waitingRow('owner/b:Q4_K_M', 2)] });
    transport.getDownloadQueue.mockResolvedValue(seeded);

    const { result } = renderHook(() => useDownloadManager());

    // No event has arrived: the running download is known from the read alone.
    await waitFor(() => expect(result.current.snapshot).toEqual(seeded));
    expect(transport.getDownloadQueue).toHaveBeenCalledTimes(1);
  });

  it('replaces the snapshot with each one the stream sends', async () => {
    const { result } = renderHook(() => useDownloadManager());
    await waitFor(() => expect(result.current.snapshot?.revision).toBe(1));

    emit(snapshotEvent(queueSnapshot({ revision: 2, active: runningRow() })));
    expect(result.current.snapshot?.active?.text.percent).toBe('25.0%');

    const later = runningRow({ percent: 50, text: { ...runningRow().text, percent: '50.0%' } });
    emit(snapshotEvent(queueSnapshot({ revision: 3, active: later })));
    expect(result.current.snapshot?.active).toEqual(later);
  });

  it('drops an older revision', async () => {
    const answerLate = heldAnswer();
    const { result } = renderHook(() => useDownloadManager());
    await waitFor(() => expect(sendEvent).toBeTruthy());

    emit(snapshotEvent(queueSnapshot({ revision: 5, active: runningRow() })));
    emit(snapshotEvent(queueSnapshot({ revision: 4 })));
    expect(result.current.snapshot?.revision).toBe(5);
    expect(result.current.snapshot?.active).toBeDefined();

    // The same snapshot again is not newer either.
    emit(snapshotEvent(queueSnapshot({ revision: 5 })));
    expect(result.current.snapshot?.active).toBeDefined();

    // Nor is the read sent at mount, answered after the stream moved on.
    await answerLate(queueSnapshot({ revision: 3 }));
    expect(result.current.snapshot?.revision).toBe(5);
  });

  it("takes a restarted daemon's first snapshot, and reads the queue again", async () => {
    const { result } = renderHook(() => useDownloadManager());
    await waitFor(() => expect(sendEvent).toBeTruthy());
    emit(snapshotEvent(queueSnapshot({ revision: 40, active: runningRow() })));
    expect(transport.getDownloadQueue).toHaveBeenCalledTimes(1);

    // The stream opens again on a daemon that counts from 1: its queue is empty.
    transport.getDownloadQueue.mockResolvedValue(queueSnapshot({ revision: 1 }));
    await act(async () => streamOpened?.());

    expect(transport.getDownloadQueue).toHaveBeenCalledTimes(2);
    expect(result.current.snapshot?.revision).toBe(1);
    expect(result.current.snapshot?.active).toBeUndefined();

    emit(snapshotEvent(queueSnapshot({ revision: 2, active: runningRow() })));
    expect(result.current.snapshot?.revision).toBe(2);
  });

  it("reports a completion in the event's own words", async () => {
    const onCompleted = vi.fn();
    renderHook(() => useDownloadManager({ onCompleted }));
    await waitFor(() => expect(sendEvent).toBeTruthy());
    const row = runningRow();
    emit(snapshotEvent(queueSnapshot({ revision: 2, active: row })));

    emit({ type: 'download_completed', id: row.id, text: 'Zeta, eight bit: in the library' });

    expect(onCompleted).toHaveBeenCalledWith({ id: row.id, text: 'Zeta, eight bit: in the library' });
  });

  it("reports a failure in the event's own words, whatever the snapshot holds", async () => {
    const onFailed = vi.fn();
    renderHook(() => useDownloadManager({ onFailed }));
    await waitFor(() => expect(sendEvent).toBeTruthy());

    emit({ type: 'download_failed', id: 'owner/b:Q4_K_M', text: 'B, four bit: it broke' });

    expect(onFailed).toHaveBeenCalledWith({ id: 'owner/b:Q4_K_M', text: 'B, four bit: it broke' });
  });

  it("keeps a run's summary until a download runs again", async () => {
    const { result } = renderHook(() => useDownloadManager());
    await waitFor(() => expect(sendEvent).toBeTruthy());
    const summary = { run_id: 'r1', items: [] } as unknown as Extract<DownloadEvent, { type: 'queue_run_complete' }>['summary'];

    emit({ type: 'queue_run_complete', summary });
    emit(snapshotEvent(queueSnapshot({ revision: 2 })));
    expect(result.current.lastQueueSummary).toBe(summary);

    emit(snapshotEvent(queueSnapshot({ revision: 3, active: runningRow() })));
    expect(result.current.lastQueueSummary).toBeNull();
  });

  it('shows a cancel as out until the download ends', async () => {
    transport.cancelDownload.mockResolvedValue(undefined);
    const { result } = renderHook(() => useDownloadManager());
    await waitFor(() => expect(sendEvent).toBeTruthy());
    const row = runningRow();
    emit(snapshotEvent(queueSnapshot({ revision: 2, active: row })));
    // The daemon has the cancel but the download has not ended yet.
    transport.getDownloadQueue.mockResolvedValue(queueSnapshot({ revision: 3, active: row }));

    await act(async () => { await result.current.cancel(row.id); });

    expect(transport.cancelDownload).toHaveBeenCalledWith(row.id);
    expect(result.current.cancellingId).toBe(row.id);

    emit({ type: 'download_cancelled', id: row.id, text: 'cancelled' });
    expect(result.current.cancellingId).toBeNull();
  });

  it('settles a cancel when the download fails, or when another is the one running', async () => {
    transport.cancelDownload.mockResolvedValue(undefined);
    const { result } = renderHook(() => useDownloadManager());
    await waitFor(() => expect(sendEvent).toBeTruthy());
    const row = runningRow();
    emit(snapshotEvent(queueSnapshot({ revision: 2, active: row })));
    transport.getDownloadQueue.mockResolvedValue(queueSnapshot({ revision: 3, active: row }));

    // The download failed before the cancel reached it: no `download_cancelled` comes.
    await act(async () => { await result.current.cancel(row.id); });
    expect(result.current.cancellingId).toBe(row.id);
    emit({ type: 'download_failed', id: row.id, text: 'failed' });
    expect(result.current.cancellingId).toBeNull();

    // The ending event was missed: a snapshot with another download running says as much.
    await act(async () => { await result.current.cancel(row.id); });
    expect(result.current.cancellingId).toBe(row.id);
    const next = runningRow({ id: 'owner/b:Q4_K_M', model_id: 'owner/b', quantization: 'Q4_K_M' });
    emit(snapshotEvent(queueSnapshot({ revision: 4, active: next })));
    expect(result.current.cancellingId).toBeNull();
  });

  it('lets a cancel be tried again when the request for it fails', async () => {
    transport.cancelDownload.mockRejectedValue(new Error('500 internal error'));
    const { result } = renderHook(() => useDownloadManager());
    await waitFor(() => expect(sendEvent).toBeTruthy());
    const row = runningRow();
    transport.getDownloadQueue.mockResolvedValue(queueSnapshot({ revision: 2, active: row }));

    await act(async () => { await result.current.cancel(row.id); });

    expect(result.current.cancellingId).toBeNull();
    expect(result.current.snapshot?.active?.id).toBe(row.id);
  });

  it('queues a model and reads the queue again', async () => {
    transport.queueDownload.mockResolvedValue({ id: 'm2:q4' });
    const { result } = renderHook(() => useDownloadManager());
    await waitFor(() => expect(result.current.snapshot?.revision).toBe(1));
    transport.getDownloadQueue.mockResolvedValue(queueSnapshot({ revision: 2, waiting: [waitingRow('m2:q4', 1)] }));

    await act(async () => { await result.current.queueModel('m2', 'q4'); });

    expect(transport.queueDownload).toHaveBeenCalledWith({ modelId: 'm2', quantization: 'q4' });
    expect(result.current.snapshot?.waiting.map((row) => row.id)).toEqual(['m2:q4']);
  });

  it('stops listening on unmount', async () => {
    const { unmount } = renderHook(() => useDownloadManager());
    await waitFor(() => expect(sendEvent).toBeTruthy());
    unmount();
    expect(unsubscribed.sort()).toEqual(['events', 'open']);
  });
});
