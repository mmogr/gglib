/**
 * What fills `downloadsRegistry`, and what a download's ending sets off.
 *
 * `bridgeEvents` over the `download` category: subscribe first, then read
 * the queue, and drop a read a snapshot event or a cleanup overtook; an
 * ending event carries no snapshot and overtakes nothing. The registry's
 * revision guard drops whatever is older still. Each time the event stream
 * opens the last revision is forgotten and the queue read again, since a
 * restarted daemon counts from 1 and keeps no backlog.
 *
 * A download that completes reads the library again, since its model is new
 * there; completions within one window are one read and one word to the
 * listeners (`onDownloadEndings`), which the toast hook turns into one toast.
 * A failure is told at once, one by one.
 *
 * @module services/downloadsEvents
 */

import { bridgeEvents } from './bridgeEvents';
import {
  forgetQueueRevision,
  markCancelling,
  resetDownloads,
  settleCancel,
  takeQueueSnapshot,
} from './downloadsRegistry';
import { refreshModels } from './modelEvents';
import { appLogger } from './platform';
import { getTransport } from './transport';
import type { DownloadCompletionInfo, DownloadFailureInfo } from './transport/types/downloads';
import type { DownloadEvent } from './transport/types/events';
import { createBatchWithinWindow, DEFAULT_BATCH_WINDOW_MS } from '../utils/batchWithinWindow';

/** Who is told how downloads ended: completions in batches, failures one by one. */
export interface DownloadEndings {
  completed: (items: DownloadCompletionInfo[]) => void;
  failed: (info: DownloadFailureInfo) => void;
}

const listeners = new Set<DownloadEndings>();

function onCompletedBurst(items: DownloadCompletionInfo[]): void {
  void refreshModels();
  listeners.forEach((l) => l.completed(items));
}

let batcher = createBatchWithinWindow<DownloadCompletionInfo>(DEFAULT_BATCH_WINDOW_MS, onCompletedBurst);

function onDownloadEvent(event: DownloadEvent): void {
  switch (event.type) {
    case 'queue_snapshot':
      takeQueueSnapshot(event);
      return;
    case 'download_completed':
      settleCancel(event.id);
      batcher.push({ id: event.id, text: event.text });
      return;
    case 'download_failed':
      settleCancel(event.id);
      listeners.forEach((l) => l.failed({ id: event.id, text: event.text }));
      return;
    case 'download_cancelled':
      settleCancel(event.id);
      return;
  }
}

const bridge = bridgeEvents({
  category: 'download',
  onEvent: (wrapped) => onDownloadEvent(wrapped.event),
  read: () => getTransport().getDownloadQueue(),
  apply: takeQueueSnapshot,
  reset: resetDownloads,
  // A snapshot's revision orders it against the read; an ending carries no
  // snapshot, and must not cost the read that may be the only one.
  overtakes: (wrapped) => wrapped.event.type === 'queue_snapshot',
});

let stopWatchingOpen: (() => void) | null = null;

/** Subscribe, read the queue, and read it again each time the stream opens. Once only. */
export function initDownloadEvents(): void {
  bridge.init();
  if (stopWatchingOpen) return;
  stopWatchingOpen = getTransport().onEventStreamOpen(() => {
    forgetQueueRevision();
    void bridge.refresh();
  });
}

/** Unsubscribe, drop a pending batch, and empty the registry. */
export function cleanupDownloadEvents(): void {
  stopWatchingOpen?.();
  stopWatchingOpen = null;
  batcher.dispose();
  batcher = createBatchWithinWindow<DownloadCompletionInfo>(DEFAULT_BATCH_WINDOW_MS, onCompletedBurst);
  bridge.cleanup();
}

/** Be told how downloads end. Returns the way to stop. */
export function onDownloadEndings(listener: DownloadEndings): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Queue `modelId` (at `quantization`, or the daemon's pick), then read the queue. */
export async function queueDownload(modelId: string, quantization?: string): Promise<void> {
  await getTransport().queueDownload({ modelId, quantization });
  await bridge.refresh();
}

/**
 * Cancel a download, once: a second cancel of the same one while the first
 * is out does nothing. It settles when the download ends or another runs;
 * a request that fails settles it at once, so it can be tried again.
 */
export async function cancelDownload(id: string): Promise<void> {
  if (!markCancelling(id)) return;
  try {
    await getTransport().cancelDownload(id);
  } catch (error) {
    // A download that ended before the cancel arrived is not found, which
    // is the outcome that was asked for.
    const gone = error instanceof Error && (error.message.includes('not found') || error.message.includes('404'));
    if (!gone) appLogger.error('service.download', 'Cancel failed', { error });
    settleCancel(id);
  } finally {
    await bridge.refresh();
  }
}

/** Take a waiting download off the queue, every file of it, then read the queue. */
export async function removeFromQueue(id: string): Promise<void> {
  try {
    await getTransport().removeFromQueue(id);
  } catch (error) {
    appLogger.error('service.download', 'Remove from queue failed', { error });
  }
  await bridge.refresh();
}
