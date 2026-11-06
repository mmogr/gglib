/**
 * The download queue, once.
 *
 * One store holds the queue as the daemon last served it, and every surface
 * that shows a download reads it: the library's Downloading group and the
 * inspector's Download buttons, which turn off while the queue is full.
 * What fills it is `downloadsEvents`: the queue route on init and whenever
 * the event stream opens, and every `queue_snapshot` event between.
 *
 * Both carry the same type from one numbered sequence, so a snapshot whose
 * `revision` is no higher than the last one taken is out of date and is
 * dropped, whichever way it came. The numbering starts again when the daemon
 * restarts, which is why `forgetQueueRevision` exists: the stream's opening
 * forgets the last number so a new daemon's first snapshot is taken.
 *
 * @module services/downloadsRegistry
 */

import { createEventStore } from './createEventStore';
import type { QueueSnapshot } from './transport/types/downloads';

export interface Downloads {
  /** The queue as the daemon last served it, or `null` before the first answer. */
  snapshot: QueueSnapshot | null;
  /** The download a cancel has been sent for and that has not yet ended. */
  cancellingId: string | null;
}

const EMPTY: Downloads = { snapshot: null, cancellingId: null };

const store = createEventStore<Downloads>(EMPTY);

/** The highest revision taken since the event stream last opened. */
let revision = 0;

/**
 * Take `next` if it is newer than the last snapshot taken. A cancel that is
 * out settles here too when the download it named is no longer the one
 * running: its ending event was missed, and the queue says as much.
 */
export function takeQueueSnapshot(next: QueueSnapshot): void {
  if (next.revision <= revision) return;
  revision = next.revision;
  const { cancellingId } = store.getState();
  const settled = cancellingId !== null && next.active?.id !== cancellingId;
  store.setState({ snapshot: next, cancellingId: settled ? null : cancellingId });
}

/** Forget the last revision: the daemon behind the stream may have restarted. */
export function forgetQueueRevision(): void {
  revision = 0;
}

/** Mark the cancel of `id` as out. Returns false when one already is. */
export function markCancelling(id: string): boolean {
  const state = store.getState();
  if (state.cancellingId === id) return false;
  store.setState({ ...state, cancellingId: id });
  return true;
}

/** The cancel of `id` is over: the download ended, or the request for it failed. */
export function settleCancel(id: string): void {
  const state = store.getState();
  if (state.cancellingId !== id) return;
  store.setState({ ...state, cancellingId: null });
}

/** Empty the store, for a bridge that is being taken down. */
export function resetDownloads(): void {
  revision = 0;
  store.setState(EMPTY);
}

/** React hook: the queue and the cancel that is out, re-rendering on every write. */
export function useDownloads(): Downloads {
  return store.useStore();
}
