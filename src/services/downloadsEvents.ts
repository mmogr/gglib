/**
 * What fills `downloadsRegistry`, and what a download's ending sets off.
 *
 * `bridgeEvents` over the `download` category: subscribe first, then read
 * the queue, and drop a read a snapshot event or a cleanup overtook; an
 * ending event carries no snapshot and overtakes nothing. The registry's
 * revision guard drops whatever is older still. Each time the event stream
 * opens the bridge reads the queue again (`keptCurrent`), and the last
 * revision is forgotten first, since a restarted daemon counts from 1 and
 * keeps no backlog.
 *
 * A download that completes reads the library again, since its model is new
 * there; completions within one window are one read and one word to the
 * listeners (`onDownloadEndings`), which the toast hook turns into one toast.
 * A failure is told at once, one by one.
 *
 * A queue request (a Download, a Retry) Hugging Face's rate limit refused
 * is held (`heldDownloads`) and sent again when the limit lifts
 * (`whenHubAllows`), so the caller can move on; one that then fails is told
 * as a failure is, its download named before the daemon's words. One the
 * queue already has by then is that download: a download's request attaches
 * to it, and an upgrade's refusal for it (a conflict) is read as so.
 *
 * @module services/downloadsEvents
 */

import { bridgeEvents } from './bridgeEvents';
import {
  forgetQueueRevision,
  isInQueue,
  markCancelling,
  resetDownloads,
  settleCancel,
  takeQueueSnapshot,
} from './downloadsRegistry';
import { dropAllHeld, heldId, holdDownload, markSending, releaseHeld, type HeldDownload } from './heldDownloads';
import { refreshModels } from './modelEvents';
import { appLogger } from './platform';
import { getTransport, TransportError } from './transport';
import { whenHubAllows } from './transport/hubLimit';
import type { DownloadCompletionInfo, DownloadFailureInfo, FinishedDownload } from './transport/types/downloads';
import type { DownloadEvent } from './transport/types/events';
import { createBatchWithinWindow, DEFAULT_BATCH_WINDOW_MS } from '../utils/batchWithinWindow';
import { formatError, isAbortError } from '../utils/errors';

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
  // A restarted daemon numbers its snapshots from 1 again.
  onStreamOpen: forgetQueueRevision,
  reset: resetDownloads,
  // A snapshot's revision orders it against the read; an ending carries no
  // snapshot, and must not cost the read that may be the only one.
  overtakes: (wrapped) => wrapped.event.type === 'queue_snapshot',
});

/** Subscribe, read the queue, and read it again each time the stream opens. Once only. */
export function initDownloadEvents(): void {
  bridge.init();
}

/** Unsubscribe, drop a pending batch and every held download, and empty the registry. */
export function cleanupDownloadEvents(): void {
  dropAllHeld();
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

/**
 * What a queue request came to: queued, with its id, or held until Hugging
 * Face's rate limit lifts, `queued` settling once it is sent again: with
 * the id, or `null` if it was dropped or then failed (told as a failure).
 */
export type Queued<Id = string> = { held: false; id: Id } | { held: true; queued: Promise<Id | null> };

/** The requests out or held, by their download's id: a second click joins the first. */
const sending = new Map<string, Promise<Queued<unknown>>>();

/**
 * Send `send` until the Hub's limit lets it through, then read the queue.
 * Answers at once once it is held; until then, as `send` does.
 */
function sendWhenAllowed<Id>(held: Omit<HeldDownload, 'liftsAt'>, send: () => Promise<Id>): Promise<Queued<Id>> {
  const already = sending.get(held.id) as Promise<Queued<Id>> | undefined;
  if (already) return already;
  const answer = new Promise<Queued<Id>>((resolve, reject) => {
    const stop = new AbortController();
    let wasHeld = false;
    const queued = whenHubAllows(send, {
      signal: stop.signal,
      onWaiting: (liftsAt) => {
        if (liftsAt === null) {
          markSending(held.id);
          return;
        }
        holdDownload({ ...held, liftsAt }, () => stop.abort());
        if (wasHeld) return;
        wasHeld = true;
        resolve({ held: true, queued });
      },
    })
      .then(
        async (id) => {
          await bridge.refresh();
          return id;
        },
        async (err: unknown) => {
          if (!wasHeld) throw err;
          if (isAbortError(err)) return null;
          if (TransportError.hasCode(err, 'CONFLICT')) {
            await bridge.refresh();
            if (isInQueue(held.id)) return held.id as Id;
          }
          listeners.forEach((l) => l.failed({ id: held.id, text: `${held.id} was not queued: ${formatError(err)}` }));
          return null;
        },
      )
      .finally(() => {
        sending.delete(held.id);
        if (wasHeld) releaseHeld(held.id);
      });
    queued.then(
      (id) => !wasHeld && resolve({ held: false, id: id as Id }),
      (err: unknown) => reject(err),
    );
  });
  sending.set(held.id, answer);
  return answer;
}

/**
 * Queue `modelId` (at `quantization`, or the daemon's pick), then read the
 * queue. Answers the download's canonical id, its row's and its finished
 * entry's `id`, or that it is held until the Hub's limit lifts.
 */
export function queueDownload(modelId: string, quantization?: string): Promise<Queued> {
  const held = { id: heldId(modelId, quantization), repo: modelId, quantization };
  return sendWhenAllowed(held, async () => (await getTransport().queueDownload({ modelId, quantization })).id);
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

/**
 * Upgrade library model `modelId` again, as a failed upgrade (`ended`) is
 * tried again: queueing its repository instead would find the old copy on
 * disk and fetch nothing. Then read the queue. Held, like a download, under
 * `ended`'s id while the Hub's limit holds it.
 */
export function upgradeAgain(ended: FinishedDownload, modelId: number): Promise<Queued<string | null>> {
  const held = { id: ended.id, repo: ended.model_id, quantization: ended.quantization };
  return sendWhenAllowed(held, async () => (await getTransport().upgradeModel(modelId)).id);
}

/**
 * Take a waiting download off the queue, every file of it, or drop a
 * finished one's entry, then read the queue.
 */
export async function removeFromQueue(id: string): Promise<void> {
  try {
    await getTransport().removeFromQueue(id);
  } catch (error) {
    appLogger.error('service.download', 'Remove from queue failed', { error });
  }
  await bridge.refresh();
}
