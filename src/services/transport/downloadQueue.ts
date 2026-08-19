/**
 * Sorting a download-queue snapshot into the three buckets the GUI reads.
 *
 * One module because a snapshot arrives two ways — the SSE `queue_snapshot`
 * frame and `GET /api/models/downloads/queue` — and each used to sort it with
 * its own copy of the rules, while a comment in the HTTP one claimed they were
 * "the same logic as the SSE event handler". They agreed on every frame the
 * server sends, so nothing was broken; they were two copies of one rule, which
 * is the state a rule is in just before it stops being one.
 *
 * # What the snapshot can actually contain
 *
 * `DownloadStatus` has seven variants, and a queue snapshot carries two of
 * them. `gglib-download` hard-codes the active row to `Downloading`
 * (`manager/mod.rs`'s `build_active_dto`) and every pending row to `Queued`
 * (`queue/mod.rs`), and failed rows are not in `items` at all — they go to a
 * separate `recent_failures` the wire type does not carry. `finalizing`,
 * `registering`, `completed`, `failed` and `cancelled` are unreachable here.
 *
 * The predicates below still name them, deliberately, and that is the only
 * claim this module makes about them: the *type* admits seven, so a reader
 * should not have to guess what a `finalizing` row would do if the queue ever
 * reported one. They are defence, not a fix — nothing today produces the
 * inputs they cover.
 *
 * @module services/transport/downloadQueue
 */

import type { DownloadQueueItem, DownloadQueueStatus } from './types/downloads';

/**
 * The queue is actively working on this row.
 *
 * `downloading` is the only one a snapshot carries today. `finalizing` and
 * `registering` are the tail of a download — bytes on disk, shards being
 * checksummed, the library row being written — and belong here rather than
 * nowhere if they ever reach a snapshot: a row in that state is work in
 * progress, and treating it as neither current nor pending would make the
 * download vanish from the UI during the phase `GlobalDownloadStatus` has
 * "Finalizing" and "Registering" labels for.
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
 * Always empty in practice — the queue keeps failures in `recent_failures`,
 * which the snapshot wire type does not carry — and `DownloadQueueStatus.failed`
 * has no reader. Both are kept because the field is part of the shape the two
 * paths return, not because either does anything.
 *
 * `cancelled` is not folded in. It is its own wire status, and a download the
 * user stopped is not one that went wrong.
 */
export function isFailed(item: DownloadQueueItem): boolean {
  return item.status === 'failed';
}

/**
 * Sort a snapshot into `{current, pending, failed}`.
 *
 * `current` is the first in-flight row. The queue runs one download at a time
 * and `get_queue_snapshot` puts the active item first, so "first" and "only"
 * coincide; a second in-flight row would be dropped rather than shown.
 */
export function bucketQueue(items: DownloadQueueItem[], maxSize = 0): DownloadQueueStatus {
  return {
    current: items.find(isInFlight) ?? null,
    pending: items.filter(isPending),
    failed: items.filter(isFailed),
    max_size: maxSize,
  };
}

/** Whether a snapshot shows the queue doing anything at all. */
export function queueIsBusy(items: DownloadQueueItem[]): boolean {
  return items.some((item) => isPending(item) || isInFlight(item));
}
