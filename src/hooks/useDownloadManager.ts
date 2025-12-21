import { useCallback, useEffect, useRef, useState } from 'react';
import {
  cancelDownload,
  cancelShardGroup,
  clearFailedDownloads,
  getDownloadQueue,
  queueDownload,
} from '../services/clients/downloads';
import { subscribeToEvent } from '../services/clients/events';
import type { DownloadQueueStatus, DownloadQueueItem, DownloadCompletionInfo, QueueDownloadResponse } from '../services/transport/types/downloads';
import type { DownloadEvent, DownloadSummary, QueueRunSummary } from '../services/transport/types/events';
import { isDesktop } from '../services/platform';

/**
 * Returns true if a queue snapshot indicates active work (busy state).
 */
function snapshotIsBusy(items: DownloadSummary[]): boolean {
  return items.some(i => i.status === 'queued' || i.status === 'downloading');
}

export type DownloadProgressStatus = 'started' | 'progress' | 'completed' | 'error';

export interface DownloadProgressView {
  status: DownloadProgressStatus;
  id: string;
  message?: string;
  downloaded?: number;
  total?: number;
  speedBps?: number;
  etaSeconds?: number;
  percentage?: number;
  shard?: {
    index: number;
    total: number;
    filename?: string;
    downloaded?: number;
    totalBytes?: number;
    aggregateDownloaded?: number;
    aggregateTotal?: number;
  } | null;
}

/**
 * Download UI state - single source of truth for what should be displayed.
 * This prevents stale progress state from keeping the UI mounted.
 */
export interface DownloadUiState {
  /** The download ID currently being displayed (null = no active download) */
  activeId: string | null;
  /** Current phase of the active download */
  phase: 'active' | 'cancelling' | null;
}

export interface UseDownloadManagerResult {
  queueStatus: DownloadQueueStatus | null;
  currentProgress: DownloadProgressView | null;
  /** Single source of truth for UI mounting/display logic */
  downloadUiState: DownloadUiState;
  /** Summary of the last completed queue run (null if no run completed or dismissed) */
  lastQueueSummary: QueueRunSummary | null;
  queueLength: number;
  connectionMode: string;
  error: string | null;
  setError: (msg: string | null) => void;
  refreshQueue: () => Promise<void>;
  queueModel: (modelId: string, quantization?: string) => Promise<QueueDownloadResponse>;
  cancel: (id: string) => Promise<void>;
  cancelGroup: (groupId: string) => Promise<void>;
  clearFailed: () => Promise<void>;
  /** Dismiss the queue run summary banner */
  clearQueueSummary: () => void;
}

const PROGRESS_THROTTLE_MS = 200;

interface UseDownloadManagerOptions {
  /**
   * Called when a download completes. Receives typed completion info
   * for the UI effects layer to handle refresh and toast logic.
   */
  onCompleted?: (info: DownloadCompletionInfo) => void;
}

function normalizeQueueItem(item: DownloadSummary): DownloadQueueItem {
  const statusMap: Record<string, 'downloading' | 'queued' | 'completed' | 'failed'> = {
    downloading: 'downloading',
    queued: 'queued',
    completed: 'completed',
    failed: 'failed',
    cancelled: 'failed',
  };

  return {
    id: item.id,
    display_name: (item as any).display_name ?? item.id,
    status: statusMap[item.status] ?? 'failed',
    position: (item as any).position ?? 0,
    error: (item as any).error,
    group_id: (item as any).group_id,
    shard_info: (item as any).shard_info,
  };
}

function snapshotToQueueStatus(items: DownloadSummary[], maxSize: number): DownloadQueueStatus {
  const normalized = items.map(normalizeQueueItem);
  const current = normalized.find((item) => item.status === 'downloading') ?? null;
  const pending = normalized.filter((item) => item.status === 'queued');
  const failed = normalized.filter((item) => item.status === 'failed');

  return { current, pending, failed, max_size: maxSize };
}

function eventToProgress(event: DownloadEvent): DownloadProgressView | null {
  switch (event.type) {
    case 'download_started':
      return { status: 'started', id: event.id };
    case 'download_progress':
      return {
        status: 'progress',
        id: event.id,
        downloaded: event.downloaded,
        total: event.total,
        speedBps: event.speed_bps,
        etaSeconds: event.eta_seconds,
        percentage: event.percentage,
      };
    case 'shard_progress':
      return {
        status: 'progress',
        id: event.id,
        percentage: event.percentage,
        speedBps: event.speed_bps,
        etaSeconds: event.eta_seconds,
        downloaded: event.aggregate_downloaded,
        total: event.aggregate_total,
        shard: {
          index: event.shard_index,
          total: event.total_shards,
          filename: event.shard_filename,
          downloaded: event.shard_downloaded,
          totalBytes: event.shard_total,
          aggregateDownloaded: event.aggregate_downloaded,
          aggregateTotal: event.aggregate_total,
        },
      };
    case 'download_completed':
      return { status: 'completed', id: event.id, message: event.message ?? 'Download completed' };
    case 'download_failed':
      return { status: 'error', id: event.id, message: event.error };
    case 'download_cancelled':
      return { status: 'error', id: event.id, message: 'Cancelled' };
    default:
      return null;
  }
}

/**
 * Extract completion info from a download ID and queue status.
 * Parses the ID format (model_id:quantization) and looks up display name from queue.
 */
function extractCompletionInfo(id: string, queueStatus: DownloadQueueStatus | null): DownloadCompletionInfo {
  // Parse ID format: "repo/model:quantization" or just "repo/model"
  const colonIndex = id.lastIndexOf(':');
  const quantization = colonIndex > 0 ? id.slice(colonIndex + 1) : undefined;
  
  // Try to find display name from current or recently completed item in queue
  const displayName = queueStatus?.current?.id === id 
    ? queueStatus.current.display_name 
    : undefined;

  return {
    modelId: id,
    quantization,
    displayName,
    source: 'huggingface', // All SSE downloads are from HuggingFace
  };
}

export function useDownloadManager(options: UseDownloadManagerOptions = {}): UseDownloadManagerResult {
  const { onCompleted } = options;
  const [queueStatus, setQueueStatus] = useState<DownloadQueueStatus | null>(null);
  const [currentProgress, setCurrentProgress] = useState<DownloadProgressView | null>(null);
  const [downloadUiState, setDownloadUiState] = useState<DownloadUiState>({
    activeId: null,
    phase: null,
  });
  const [lastQueueSummary, setLastQueueSummary] = useState<QueueRunSummary | null>(null);
  const [connectionMode, setConnectionMode] = useState<string>('Initializing...');
  const [error, setError] = useState<string | null>(null);

  const lastProgressUpdateRef = useRef<number>(0);
  const pendingProgressRef = useRef<DownloadProgressView | null>(null);
  const throttleTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const cancelInFlightRef = useRef<Set<string>>(new Set());
  
  // Ref to access current queue status in event handler without causing re-subscriptions
  const queueStatusRef = useRef<DownloadQueueStatus | null>(null);
  queueStatusRef.current = queueStatus;

  // Ref to access current summary in event handler
  const lastQueueSummaryRef = useRef<QueueRunSummary | null>(null);
  lastQueueSummaryRef.current = lastQueueSummary;

  // Ref to access current UI state in callbacks
  const downloadUiStateRef = useRef<DownloadUiState>(downloadUiState);
  downloadUiStateRef.current = downloadUiState;

  /**
   * Terminal cleanup - clears all download UI state and stops any polling/timers.
   * This is the single point of cleanup to prevent stuck progress bars.
   */
  const cleanupTerminal = useCallback((id: string) => {
    // Only cleanup if this is the currently active download
    if (downloadUiStateRef.current.activeId === id) {
      setDownloadUiState({ activeId: null, phase: null });
      setCurrentProgress(null);
      
      // Clear any pending throttled progress updates
      if (throttleTimeoutRef.current) {
        clearTimeout(throttleTimeoutRef.current);
        throttleTimeoutRef.current = null;
      }
      pendingProgressRef.current = null;
    }
    // Remove from in-flight cancel tracking
    cancelInFlightRef.current.delete(id);
  }, []);

  const throttledSetProgress = useCallback((progressData: DownloadProgressView) => {
    const now = Date.now();
    const elapsed = now - lastProgressUpdateRef.current;

    if (progressData.status !== 'progress') {
      lastProgressUpdateRef.current = now;
      setCurrentProgress(progressData);
      pendingProgressRef.current = null;
      return;
    }

    if (elapsed >= PROGRESS_THROTTLE_MS) {
      lastProgressUpdateRef.current = now;
      setCurrentProgress(progressData);
      pendingProgressRef.current = null;
      return;
    }

    pendingProgressRef.current = progressData;
    if (!throttleTimeoutRef.current) {
      throttleTimeoutRef.current = setTimeout(() => {
        if (pendingProgressRef.current) {
          lastProgressUpdateRef.current = Date.now();
          setCurrentProgress(pendingProgressRef.current);
          pendingProgressRef.current = null;
        }
        throttleTimeoutRef.current = null;
      }, PROGRESS_THROTTLE_MS - elapsed);
    }
  }, []);

  // Use refs to avoid re-creating event handler and causing subscription loops
  const throttledSetProgressRef = useRef(throttledSetProgress);
  throttledSetProgressRef.current = throttledSetProgress;
  
  const onCompletedRef = useRef(onCompleted);
  onCompletedRef.current = onCompleted;

  useEffect(() => {
    return () => {
      if (throttleTimeoutRef.current) {
        clearTimeout(throttleTimeoutRef.current);
      }
    };
  }, []);

  const refreshQueue = useCallback(async () => {
    try {
      const snapshot = await getDownloadQueue();
      setQueueStatus(snapshot);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to load queue');
    }
  }, []);

  useEffect(() => {
    refreshQueue();

    // Stable event handler that reads from refs
    const handleEvent = (wrappedEvent: { type: 'download'; event: DownloadEvent }) => {
      // Unwrap the download event from the AppEvent wrapper
      const event = wrappedEvent.event;
      
      if (event.type === 'queue_snapshot') {
        const snapshot = snapshotToQueueStatus(event.items, event.max_size);
        setQueueStatus(snapshot);
        
        // Clear old summary when new run starts (prevent stale success banner)
        if (snapshotIsBusy(event.items) && lastQueueSummaryRef.current) {
          setLastQueueSummary(null);
        }
        
        // Update activeId based on queue state
        if (snapshot.current) {
          setDownloadUiState(prev => ({
            activeId: snapshot.current!.id,
            phase: prev.phase === 'cancelling' ? 'cancelling' : 'active',
          }));
        } else if (downloadUiStateRef.current.phase !== 'cancelling') {
          // Queue is empty and not cancelling - cleanup
          setDownloadUiState({ activeId: null, phase: null });
          setCurrentProgress(null);
        }
        return;
      }

      if (event.type === 'queue_run_complete') {
        // Queue run finished - store summary for banner display
        setLastQueueSummary(event.summary);
        return;
      }

      const progress = eventToProgress(event);
      if (progress) {
        throttledSetProgressRef.current(progress);

        // Ensure activeId is set for any progress event (prevents snapshot lag issues)
        if (downloadUiStateRef.current.activeId !== progress.id && 
            downloadUiStateRef.current.phase !== 'cancelling') {
          setDownloadUiState({
            activeId: progress.id,
            phase: 'active',
          });
        }

        if (progress.status === 'completed' && event.type === 'download_completed') {
          // Extract completion info from event for UI effects layer
          const completionInfo = extractCompletionInfo(event.id, queueStatusRef.current);
          onCompletedRef.current?.(completionInfo);
          // Keep completed state visible briefly, then cleanup
          setTimeout(() => cleanupTerminal(event.id), 2000);
        } else if (event.type === 'download_cancelled' || event.type === 'download_failed') {
          // Terminal events: cleanup immediately
          cleanupTerminal(event.id);
        }
      }
    };

    // Subscribe to download events via Transport (sync unsubscribe)
    const unsubscribe = subscribeToEvent('download', handleEvent);
    setConnectionMode(isDesktop() ? 'Desktop (Tauri)' : 'Web (SSE)');

    return () => {
      unsubscribe();
    };
  }, [refreshQueue, cleanupTerminal]); // Only depend on refreshQueue, not handleEvent

  const queueModel = useCallback(async (modelId: string, quantization?: string) => {
    const response = await queueDownload({ modelId, quantization });
    await refreshQueue();
    return response;
  }, [refreshQueue]);

  const cancel = useCallback(async (id: string) => {
    // Guard: prevent duplicate cancel calls for same ID
    if (cancelInFlightRef.current.has(id)) {
      return;
    }
    
    cancelInFlightRef.current.add(id);
    
    // Optimistic UI: immediately show cancelling state
    setDownloadUiState(prev => ({
      ...prev,
      phase: 'cancelling',
    }));
    
    try {
      await cancelDownload(id);
      // Success - cleanup will happen via SSE event or we'll do it now
      cleanupTerminal(id);
    } catch (error) {
      // Treat 404 as success (idempotent cancel)
      const is404 = error instanceof Error && 
        (error.message.includes('not found') || error.message.includes('404'));
      
      if (is404) {
        // Expected race condition - cleanup
        cleanupTerminal(id);
      } else {
        // Unexpected error - still cleanup UI to prevent stuck state
        console.error('Cancel failed:', error);
        cleanupTerminal(id);
        // Could show error toast here if desired
      }
    } finally {
      // Always refresh queue as best-effort
      try {
        await refreshQueue();
      } catch (e) {
        // Ignore refresh errors - don't let them break cleanup
        console.warn('Failed to refresh queue after cancel:', e);
      }
    }
  }, [refreshQueue, cleanupTerminal]);

  const cancelGroup = useCallback(async (groupId: string) => {
    await cancelShardGroup(groupId);
    await refreshQueue();
  }, [refreshQueue]);

  const clearFailed = useCallback(async () => {
    await clearFailedDownloads();
    await refreshQueue();
  }, [refreshQueue]);

  const clearQueueSummary = useCallback(() => {
    setLastQueueSummary(null);
  }, []);

  // Calculate queue length: pending items + 1 if there's an active download
  const queueLength = (queueStatus?.pending?.length || 0) + (downloadUiState.activeId ? 1 : 0);

  return {
    queueStatus,
    currentProgress,
    downloadUiState,
    lastQueueSummary,
    queueLength,
    connectionMode,
    error,
    setError,
    refreshQueue,
    queueModel,
    cancel,
    cancelGroup,
    clearFailed,
    clearQueueSummary,
  };
}
