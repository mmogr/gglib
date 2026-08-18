/**
 * Sorting a flat download-queue snapshot into the three buckets the GUI reads.
 *
 * One module because there are two ways a snapshot arrives — the SSE
 * `queue_snapshot` frame and `GET /api/models/downloads/queue` — and they used
 * to sort it differently while a comment in the HTTP path claimed they were
 * "the same logic as the SSE event handler". They were not: the SSE side ran
 * every row through a status map that collapsed seven wire states into four,
 * and the HTTP side used the raw status. A user could see a different queue
 * depending on which one answered last.
 *
 * @module services/transport/downloadQueue
 */

import type { DownloadQueueItem, DownloadQueueStatus } from './types/downloads';

/**
 * The queue is actively working on this row.
 *
 * Three statuses and not one: `finalizing` and `registering` are the tail of a
 * download — the bytes are on disk and the queue is checksumming shards and
 * writing the library row. Treating only `downloading` as in-flight makes a
 * download disappear from the UI for those two phases, which is precisely
 * when `GlobalDownloadStatus` wants to say "Finalizing" and "Registering".
 */
export function isInFlight(item: DownloadQueueItem): boolean {
  return (
    item.status === 'downloading' || item.status === 'finalizing' || item.status === 'registering'
  );
}

/** Waiting for a slot; nothing has been fetched yet. */
export function isPending(item: DownloadQueueItem): boolean {
  return item.status === 'queued';
}

/**
 * Ended badly.
 *
 * `cancelled` is deliberately not folded in. It is its own wire status, and a
 * download the user stopped is not one that went wrong.
 */
export function isFailed(item: DownloadQueueItem): boolean {
  return item.status === 'failed';
}

/**
 * Sort a snapshot into `{current, pending, failed}`.
 *
 * `current` is the first in-flight row. The queue runs one download at a time,
 * so "first" and "only" coincide; taking the first keeps that assumption in
 * one place rather than asserting it.
 */
export function bucketQueue(
  items: DownloadQueueItem[],
  maxSize = 0,
): DownloadQueueStatus {
  return {
    current: items.find(isInFlight) ?? null,
    pending: items.filter(isPending),
    failed: items.filter(isFailed),
    max_size: maxSize,
  };
}

/**
 * Whether a snapshot shows the queue doing anything at all.
 *
 * Includes the finalize and register tail, so a success banner is not cleared
 * while the queue is still writing the row it is about to announce.
 */
export function queueIsBusy(items: DownloadQueueItem[]): boolean {
  return items.some((item) => isPending(item) || isInFlight(item));
}
