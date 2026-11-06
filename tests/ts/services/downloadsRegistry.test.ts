/**
 * The download queue the GUI holds is the daemon's snapshot, taken whole,
 * and a download's ending is said once, on any screen.
 *
 * The snapshot arrives two ways, from `GET /api/models/downloads/queue` and
 * in `queue_snapshot` events, numbered in one sequence. What is pinned here
 * is which snapshot wins: the one with the higher `revision`, until the
 * event stream opens again, when the numbering may have started over. Then
 * the cancel that is out and when it settles, and the endings: completions
 * batched into one library read and one toast, failures told at once.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { useDownloads } from '../../../src/services/downloadsRegistry';
import {
  cancelDownload,
  cleanupDownloadEvents,
  initDownloadEvents,
  queueDownload,
  removeFromQueue,
} from '../../../src/services/downloadsEvents';
import { useDownloadToasts } from '../../../src/hooks/useDownloadToasts';
import { ToastProvider, useToastContext } from '../../../src/contexts/ToastContext';
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
  removeFromQueue: vi.fn(),
  subscribe: vi.fn(),
  onEventStreamOpen: vi.fn(),
}));
const refreshModels = vi.hoisted(() => vi.fn(async () => {}));

vi.mock('../../../src/services/transport', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../../../src/services/transport')>()),
  getTransport: () => transport,
}));
vi.mock('../../../src/services/modelEvents', () => ({ refreshModels }));

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

/** The registry, read as a screen reads it, with the bridge up. */
function watch() {
  initDownloadEvents();
  return renderHook(() => useDownloads());
}

/** The toasts the hook raises, under the app's provider. */
function watchToasts() {
  return renderHook(() => {
    useDownloadToasts();
    return useToastContext().toasts.map((t) => [t.message, t.type]);
  }, { wrapper: ToastProvider });
}

const completed = (id: string, text: string): DownloadEvent => ({ type: 'download_completed', id, text });

describe('downloadsRegistry', () => {
  beforeEach(() => {
    sendEvent = null;
    streamOpened = null;
    unsubscribed = [];
    for (const mock of Object.values(transport)) mock.mockReset();
    refreshModels.mockClear();
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
  afterEach(() => {
    cleanupDownloadEvents();
    vi.useRealTimers();
  });

  it('subscribes before it reads, and seeds from the read', async () => {
    const seeded = queueSnapshot({ revision: 7, active: runningRow(), waiting: [waitingRow('owner/b:Q4_K_M', 2)] });
    transport.getDownloadQueue.mockImplementation(async () => {
      expect(transport.subscribe).toHaveBeenCalled();
      return seeded;
    });

    const { result } = watch();

    // No event has arrived: the running download is known from the read alone.
    await waitFor(() => expect(result.current.snapshot).toEqual(seeded));
    expect(transport.getDownloadQueue).toHaveBeenCalledTimes(1);
  });

  it('replaces the snapshot with each one the stream sends', async () => {
    const { result } = watch();
    await waitFor(() => expect(result.current.snapshot?.revision).toBe(1));

    emit(snapshotEvent(queueSnapshot({ revision: 2, active: runningRow() })));
    expect(result.current.snapshot?.active?.text.percent).toBe('25.0%');

    const later = runningRow({ percent: 50, text: { ...runningRow().text, percent: '50.0%' } });
    emit(snapshotEvent(queueSnapshot({ revision: 3, active: later })));
    expect(result.current.snapshot?.active).toEqual(later);
  });

  it('drops an older revision', async () => {
    const answerLate = heldAnswer();
    const { result } = watch();

    emit(snapshotEvent(queueSnapshot({ revision: 5, active: runningRow() })));
    emit(snapshotEvent(queueSnapshot({ revision: 4 })));
    expect(result.current.snapshot?.revision).toBe(5);
    expect(result.current.snapshot?.active).toBeDefined();

    // The same snapshot again is not newer either.
    emit(snapshotEvent(queueSnapshot({ revision: 5 })));
    expect(result.current.snapshot?.active).toBeDefined();

    // Nor is the read sent at init, answered after the stream moved on.
    await answerLate(queueSnapshot({ revision: 3 }));
    expect(result.current.snapshot?.revision).toBe(5);
  });

  it('keeps its first read over an ending that carries no snapshot', async () => {
    const answer = heldAnswer();
    const { result } = watch();

    emit({ type: 'download_completed', id: 'owner/a:Q4_K_M', text: 'A: downloaded' });
    await answer(queueSnapshot({ revision: 4, waiting: [waitingRow('owner/b:Q4_K_M', 1)] }));

    expect(result.current.snapshot?.revision).toBe(4);
  });

  it("takes a restarted daemon's first snapshot, and reads the queue again", async () => {
    const { result } = watch();
    await waitFor(() => expect(result.current.snapshot?.revision).toBe(1));
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

  it('shows a cancel as out until the download ends', async () => {
    transport.cancelDownload.mockResolvedValue(undefined);
    const { result } = watch();
    await waitFor(() => expect(result.current.snapshot).not.toBeNull());
    const row = runningRow();
    emit(snapshotEvent(queueSnapshot({ revision: 2, active: row })));
    // The daemon has the cancel but the download has not ended yet.
    transport.getDownloadQueue.mockResolvedValue(queueSnapshot({ revision: 3, active: row }));

    await act(() => cancelDownload(row.id));

    expect(transport.cancelDownload).toHaveBeenCalledWith(row.id);
    expect(result.current.cancellingId).toBe(row.id);

    // A second cancel while the first is out sends nothing.
    await act(() => cancelDownload(row.id));
    expect(transport.cancelDownload).toHaveBeenCalledTimes(1);

    emit({ type: 'download_cancelled', id: row.id, text: 'cancelled' });
    expect(result.current.cancellingId).toBeNull();
  });

  it('settles a cancel when the download fails, or when another is the one running', async () => {
    transport.cancelDownload.mockResolvedValue(undefined);
    const { result } = watch();
    await waitFor(() => expect(result.current.snapshot).not.toBeNull());
    const row = runningRow();
    emit(snapshotEvent(queueSnapshot({ revision: 2, active: row })));
    transport.getDownloadQueue.mockResolvedValue(queueSnapshot({ revision: 3, active: row }));

    // The download failed before the cancel reached it: no `download_cancelled` comes.
    await act(() => cancelDownload(row.id));
    expect(result.current.cancellingId).toBe(row.id);
    emit({ type: 'download_failed', id: row.id, text: 'failed' });
    expect(result.current.cancellingId).toBeNull();

    // The ending event was missed: a snapshot with another download running says as much.
    await act(() => cancelDownload(row.id));
    expect(result.current.cancellingId).toBe(row.id);
    const next = runningRow({ id: 'owner/b:Q4_K_M', model_id: 'owner/b', quantization: 'Q4_K_M' });
    emit(snapshotEvent(queueSnapshot({ revision: 4, active: next })));
    expect(result.current.cancellingId).toBeNull();
  });

  it('lets a cancel be tried again when the request for it fails', async () => {
    transport.cancelDownload.mockRejectedValue(new Error('500 internal error'));
    const { result } = watch();
    await waitFor(() => expect(result.current.snapshot).not.toBeNull());
    const row = runningRow();
    transport.getDownloadQueue.mockResolvedValue(queueSnapshot({ revision: 2, active: row }));

    await act(() => cancelDownload(row.id));

    expect(result.current.cancellingId).toBeNull();
    expect(result.current.snapshot?.active?.id).toBe(row.id);
  });

  it('queues a model, or takes a waiting one off, and reads the queue again', async () => {
    transport.queueDownload.mockResolvedValue({ id: 'm2:q4' });
    const { result } = watch();
    await waitFor(() => expect(result.current.snapshot?.revision).toBe(1));
    transport.getDownloadQueue.mockResolvedValue(queueSnapshot({ revision: 2, waiting: [waitingRow('m2:q4', 1)] }));

    await act(() => queueDownload('m2', 'q4'));

    expect(transport.queueDownload).toHaveBeenCalledWith({ modelId: 'm2', quantization: 'q4' });
    expect(result.current.snapshot?.waiting.map((row) => row.id)).toEqual(['m2:q4']);

    transport.removeFromQueue.mockResolvedValue(undefined);
    transport.getDownloadQueue.mockResolvedValue(queueSnapshot({ revision: 3 }));
    await act(() => removeFromQueue('m2:q4'));

    expect(transport.removeFromQueue).toHaveBeenCalledWith('m2:q4');
    expect(result.current.snapshot?.waiting).toEqual([]);
  });

  it('stops listening, and empties, on cleanup', async () => {
    const { result } = watch();
    await waitFor(() => expect(result.current.snapshot).not.toBeNull());
    act(() => cleanupDownloadEvents());
    expect(unsubscribed.sort()).toEqual(['events', 'open']);
    expect(result.current.snapshot).toBeNull();
  });

  describe('endings', () => {
    it("says one download in the daemon's own words, and reads the library once", async () => {
      const text = 'owner/zeta-GGUF:Q8_0: Downloaded model to models/zeta.Q8_0.gguf. '
        + 'zeta.Q8_0.gguf was added without its details: Invalid GGUF format: Invalid magic number';
      const { result } = watchToasts();
      watch();
      vi.useFakeTimers();

      emit(completed('owner/zeta-GGUF:Q8_0', text));
      expect(refreshModels).not.toHaveBeenCalled();
      act(() => vi.advanceTimersByTime(350));

      expect(result.current).toEqual([[text, 'success']]);
      expect(refreshModels).toHaveBeenCalledTimes(1);
    });

    it('counts the downloads that end within one window: one toast, one read', () => {
      const { result } = watchToasts();
      watch();
      vi.useFakeTimers();

      emit(completed('a', 'A: downloaded'));
      emit(completed('b', 'B: downloaded'));
      act(() => vi.advanceTimersByTime(349));
      expect(result.current).toEqual([]);
      emit(completed('c', 'C: downloaded'));
      act(() => vi.advanceTimersByTime(1));

      expect(result.current).toEqual([['3 models downloaded', 'success']]);
      expect(refreshModels).toHaveBeenCalledTimes(1);
    });

    it('says downloads in different windows separately', () => {
      const { result } = watchToasts();
      watch();
      vi.useFakeTimers();

      emit(completed('a', 'A: downloaded'));
      emit(completed('b', 'B: downloaded'));
      act(() => vi.advanceTimersByTime(350));
      emit(completed('c', 'C: downloaded'));
      act(() => vi.advanceTimersByTime(350));

      expect(result.current).toEqual([['2 models downloaded', 'success'], ['C: downloaded', 'success']]);
      expect(refreshModels).toHaveBeenCalledTimes(2);
    });

    it("raises an error toast at once for a failure, in the daemon's words, and reads nothing", () => {
      const { result } = watchToasts();
      watch();

      emit({ type: 'download_failed', id: 'owner/b:Q4_K_M', text: 'B, four bit: it broke' });

      expect(result.current).toEqual([['B, four bit: it broke', 'error']]);
      expect(refreshModels).not.toHaveBeenCalled();
    });

    it('drops a pending batch on cleanup', () => {
      const { result } = watchToasts();
      watch();
      vi.useFakeTimers();

      emit(completed('a', 'A: downloaded'));
      act(() => cleanupDownloadEvents());
      act(() => vi.advanceTimersByTime(350));

      expect(result.current).toEqual([]);
      expect(refreshModels).not.toHaveBeenCalled();
    });
  });
});
