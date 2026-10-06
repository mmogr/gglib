import { useCallback, useEffect, useRef, useState } from 'react';
import { appLogger } from '../services/platform';
import type {
  DownloadCompletionInfo,
  DownloadFailureInfo,
  QueueDownloadResponse,
  QueueSnapshot,
} from '../services/transport/types/downloads';
import type { DownloadEvent, QueueRunSummary } from '../services/transport/types/events';
import { getTransport } from '../services/transport';

export interface UseDownloadManagerResult {
  /**
   * The download queue as the daemon last served it, or null before the
   * first answer. This is the whole of the download state: the rows are
   * drawn from it as they are.
   */
  snapshot: QueueSnapshot | null;
  /** The download a cancel has been sent for and that has not yet ended. */
  cancellingId: string | null;
  /** Summary of the last completed queue run (null if no run completed or dismissed) */
  lastQueueSummary: QueueRunSummary | null;
  error: string | null;
  setError: (msg: string | null) => void;
  refreshQueue: () => Promise<void>;
  queueModel: (modelId: string, quantization?: string) => Promise<QueueDownloadResponse>;
  cancel: (id: string) => Promise<void>;
  clearFailed: () => Promise<void>;
  /** Dismiss the queue run summary banner */
  clearQueueSummary: () => void;
}

interface UseDownloadManagerOptions {
  /** Called when a download completes, for the model refresh and the toast. */
  onCompleted?: (info: DownloadCompletionInfo) => void;
  /** Called when a download fails, for the toast. */
  onFailed?: (info: DownloadFailureInfo) => void;
}

/**
 * The name the queue last gave the download `id`.
 *
 * A download that just ended is still the snapshot's active row: its ending
 * is sent before the snapshot that moves it to `finished`. With no snapshot
 * that names it, the id stands in.
 */
function titleOf(snapshot: QueueSnapshot | null, id: string): string {
  if (snapshot?.active?.id === id) return snapshot.active.text.title;
  const row = snapshot?.waiting.find((waiting) => waiting.id === id);
  if (row) return row.text.title;
  return snapshot?.finished.find((ended) => ended.id === id)?.title ?? id;
}

/**
 * The download queue, held as the daemon's own snapshot.
 *
 * The state is seeded from `GET /api/models/downloads/queue` on mount and
 * replaced by every `queue_snapshot` event. Both carry the same type from one
 * numbered sequence, so a snapshot with a `revision` no higher than the last
 * one taken is out of date and is dropped, whichever way it came.
 *
 * The numbering starts again when the daemon restarts. Each time the event
 * stream opens the last revision is forgotten and the queue is read again, so
 * a new daemon's first snapshot is taken rather than dropped as old.
 */
export function useDownloadManager(options: UseDownloadManagerOptions = {}): UseDownloadManagerResult {
  const { onCompleted, onFailed } = options;
  const [snapshot, setSnapshot] = useState<QueueSnapshot | null>(null);
  const [cancellingId, setCancellingId] = useState<string | null>(null);
  const [lastQueueSummary, setLastQueueSummary] = useState<QueueRunSummary | null>(null);
  const [error, setError] = useState<string | null>(null);

  // The snapshot the handlers below read, kept beside the state so that two
  // snapshots arriving before a render are still compared with each other.
  const snapshotRef = useRef<QueueSnapshot | null>(null);
  // The highest revision taken since the event stream last opened.
  const revisionRef = useRef(0);
  // The download being cancelled, for the handlers; `cancellingId` is its copy
  // for rendering.
  const cancellingRef = useRef<string | null>(null);

  const onCompletedRef = useRef(onCompleted);
  onCompletedRef.current = onCompleted;
  const onFailedRef = useRef(onFailed);
  onFailedRef.current = onFailed;

  /** The cancel of `id` is over: it ended, or the request for it failed. */
  const cancelSettled = useCallback((id: string) => {
    if (cancellingRef.current !== id) return;
    cancellingRef.current = null;
    setCancellingId(null);
  }, []);

  const takeSnapshot = useCallback((next: QueueSnapshot) => {
    if (next.revision <= revisionRef.current) return;
    revisionRef.current = next.revision;
    snapshotRef.current = next;
    setSnapshot(next);
    // A download is running, so a banner about the run before it is stale.
    if (next.active) setLastQueueSummary(null);
    // The download being cancelled is no longer the one running.
    if (cancellingRef.current && next.active?.id !== cancellingRef.current) {
      cancelSettled(cancellingRef.current);
    }
  }, [cancelSettled]);

  const refreshQueue = useCallback(async () => {
    try {
      takeSnapshot(await getTransport().getDownloadQueue());
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to load queue');
    }
  }, [takeSnapshot]);

  useEffect(() => {
    refreshQueue();

    const handleEvent = (wrapped: { type: 'download'; event: DownloadEvent }) => {
      const event = wrapped.event;
      switch (event.type) {
        case 'queue_snapshot':
          takeSnapshot(event);
          return;
        case 'queue_run_complete':
          setLastQueueSummary(event.summary);
          return;
        case 'download_completed':
          cancelSettled(event.id);
          onCompletedRef.current?.({ id: event.id, title: titleOf(snapshotRef.current, event.id) });
          return;
        case 'download_failed':
          cancelSettled(event.id);
          onFailedRef.current?.({
            id: event.id,
            title: titleOf(snapshotRef.current, event.id),
            error: event.error,
          });
          return;
        case 'download_cancelled':
          cancelSettled(event.id);
          return;
      }
    };

    const transport = getTransport();
    const unsubscribe = transport.subscribe('download', handleEvent);
    const stopWatchingOpen = transport.onEventStreamOpen(() => {
      revisionRef.current = 0;
      refreshQueue();
    });

    return () => {
      unsubscribe();
      stopWatchingOpen();
    };
  }, [refreshQueue, takeSnapshot, cancelSettled]);

  const queueModel = useCallback(async (modelId: string, quantization?: string) => {
    const response = await getTransport().queueDownload({ modelId, quantization });
    await refreshQueue();
    return response;
  }, [refreshQueue]);

  const cancel = useCallback(async (id: string) => {
    // One cancel per download: the button is disabled while this one is out.
    if (cancellingRef.current === id) return;
    cancellingRef.current = id;
    setCancellingId(id);

    try {
      await getTransport().cancelDownload(id);
      // The download ends a moment later. Its ending, or a snapshot without
      // it, settles the cancel.
    } catch (error) {
      // A download that ended before the cancel arrived is not found, which
      // is the outcome that was asked for.
      const gone = error instanceof Error &&
        (error.message.includes('not found') || error.message.includes('404'));
      if (!gone) {
        appLogger.error('hook.download', 'Cancel failed', { error });
      }
      cancelSettled(id);
    } finally {
      await refreshQueue();
    }
  }, [refreshQueue, cancelSettled]);

  const clearFailed = useCallback(async () => {
    await getTransport().clearFailedDownloads();
    await refreshQueue();
  }, [refreshQueue]);

  const clearQueueSummary = useCallback(() => {
    setLastQueueSummary(null);
  }, []);

  return {
    snapshot,
    cancellingId,
    lastQueueSummary,
    error,
    setError,
    refreshQueue,
    queueModel,
    cancel,
    clearFailed,
    clearQueueSummary,
  };
}
