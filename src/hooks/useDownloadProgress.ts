import { useState, useEffect, useCallback, useRef } from 'react';
import { getDownloadQueue, cancelDownload as cancelDownloadService } from '../services/tauri';
import { isTauriApp } from '../utils/platform';
import { DownloadQueueStatus, DownloadEvent, DownloadSummary, DownloadQueueItem } from '../types';

export interface ShardProgressInfo {
  current_shard: number;
  total_shards: number;
  current_filename: string;
  shard_downloaded: number;
  shard_total: number;
  aggregate_downloaded: number;
  aggregate_total: number;
}

export interface DownloadProgress {
  status: 'started' | 'downloading' | 'progress' | 'completed' | 'error' | 'queued' | 'skipped';
  model_id: string;
  message?: string;
  progress?: number;
  downloaded?: number;
  total?: number;
  percentage?: number;
  speed?: number;
  eta?: number;
  queue_position?: number;
  queue_length?: number;
  shard_progress?: ShardProgressInfo | null;
}

interface UseDownloadProgressOptions {
  onCompleted?: () => void;
}

interface UseDownloadProgressReturn {
  progress: DownloadProgress | null;
  queueStatus: DownloadQueueStatus | null;
  connectionMode: string;
  error: string | null;
  setError: (error: string | null) => void;
  clearProgress: () => void;
  fetchQueueStatus: () => Promise<void>;
  cancelDownload: (modelId: string) => Promise<void>;
  isDownloading: boolean;
  queueCount: number;
}

// Throttle interval for progress updates (ms)
const PROGRESS_THROTTLE_MS = 200;

/**
 * Convert a DownloadSummary (from backend) to DownloadQueueItem (for frontend state).
 */
function summaryToQueueItem(summary: DownloadSummary): DownloadQueueItem {
  // Map backend status to frontend status
  const statusMap: Record<string, DownloadQueueItem['status']> = {
    queued: 'queued',
    downloading: 'downloading',
    completed: 'completed',
    failed: 'failed',
    cancelled: 'failed', // Treat cancelled as failed for UI purposes
  };

  return {
    id: summary.id,
    display_name: summary.display_name,
    status: statusMap[summary.status] || 'queued',
    position: summary.position,
    error: summary.error,
    group_id: summary.group_id,
    shard_info: summary.shard_info,
  };
}

/**
 * Convert a DownloadEvent from the backend to the internal DownloadProgress format.
 */
function eventToProgress(event: DownloadEvent): DownloadProgress | null {
  switch (event.type) {
    case 'download_started':
      return {
        status: 'started',
        model_id: event.id,
      };

    case 'download_progress':
      return {
        status: 'progress',
        model_id: event.id,
        downloaded: event.downloaded,
        total: event.total,
        percentage: event.percentage,
        speed: event.speed_bps,
        eta: event.eta_seconds,
      };

    case 'shard_progress':
      return {
        status: 'progress',
        model_id: event.id,
        downloaded: event.aggregate_downloaded,
        total: event.aggregate_total,
        percentage: event.percentage,
        speed: event.speed_bps,
        eta: event.eta_seconds,
        shard_progress: {
          current_shard: event.shard_index,
          total_shards: event.total_shards,
          current_filename: event.shard_filename,
          shard_downloaded: event.shard_downloaded,
          shard_total: event.shard_total,
          aggregate_downloaded: event.aggregate_downloaded,
          aggregate_total: event.aggregate_total,
        },
      };

    case 'download_completed':
      return {
        status: 'completed',
        model_id: event.id,
        message: event.message ?? undefined,
      };

    case 'download_failed':
      return {
        status: 'error',
        model_id: event.id,
        message: event.error,
      };

    case 'download_cancelled':
      return {
        status: 'error',
        model_id: event.id,
        message: 'Download cancelled',
      };

    case 'queue_snapshot':
      // Queue snapshots don't produce a progress event
      return null;

    default:
      return null;
  }
}

/**
 * Convert a queue_snapshot event to DownloadQueueStatus.
 */
function snapshotToQueueStatus(items: DownloadSummary[], maxSize: number): DownloadQueueStatus {
  const queueItems = items.map(summaryToQueueItem);
  
  // Find current (downloading) item
  const current = queueItems.find(item => item.status === 'downloading') ?? null;
  
  // Pending items (queued, not yet started)
  const pending = queueItems.filter(item => item.status === 'queued');
  
  // Failed items
  const failed = queueItems.filter(item => item.status === 'failed');

  return {
    current,
    pending,
    failed,
    max_size: maxSize,
  };
}

/**
 * Hook to listen for download progress events from Tauri or Web SSE.
 * Reusable across components that need to show download progress.
 */
export function useDownloadProgress(options: UseDownloadProgressOptions = {}): UseDownloadProgressReturn {
  const { onCompleted } = options;
  
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [queueStatus, setQueueStatus] = useState<DownloadQueueStatus | null>(null);
  const [connectionMode, setConnectionMode] = useState<string>('Initializing...');
  const [error, setError] = useState<string | null>(null);
  
  // Throttle refs for progress updates
  const lastProgressUpdateRef = useRef<number>(0);
  const pendingProgressRef = useRef<DownloadProgress | null>(null);
  const throttleTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Throttled progress setter to prevent excessive re-renders
  const throttledSetProgress = useCallback((progressData: DownloadProgress) => {
    const now = Date.now();
    const timeSinceLastUpdate = now - lastProgressUpdateRef.current;

    // Always update immediately for non-progress events (completed, error, etc.)
    if (progressData.status !== 'progress') {
      lastProgressUpdateRef.current = now;
      setProgress(progressData);
      return;
    }

    // If enough time has passed, update immediately
    if (timeSinceLastUpdate >= PROGRESS_THROTTLE_MS) {
      lastProgressUpdateRef.current = now;
      setProgress(progressData);
      pendingProgressRef.current = null;
      return;
    }

    // Otherwise, store the latest progress and schedule an update
    pendingProgressRef.current = progressData;
    
    if (!throttleTimeoutRef.current) {
      throttleTimeoutRef.current = setTimeout(() => {
        if (pendingProgressRef.current) {
          lastProgressUpdateRef.current = Date.now();
          setProgress(pendingProgressRef.current);
          pendingProgressRef.current = null;
        }
        throttleTimeoutRef.current = null;
      }, PROGRESS_THROTTLE_MS - timeSinceLastUpdate);
    }
  }, []);

  // Handle a DownloadEvent from the backend
  const handleDownloadEvent = useCallback((event: DownloadEvent) => {
    // Handle queue_snapshot specially - update queue status
    if (event.type === 'queue_snapshot') {
      setQueueStatus(snapshotToQueueStatus(event.items, event.max_size));
      return;
    }

    // Convert other events to progress
    const progressData = eventToProgress(event);
    if (progressData) {
      throttledSetProgress(progressData);

      if (progressData.status === 'completed') {
        onCompleted?.();
        setTimeout(() => {
          setProgress(null);
        }, 2000);
      }
    }
  }, [throttledSetProgress, onCompleted]);

  // Cleanup throttle timeout on unmount
  useEffect(() => {
    return () => {
      if (throttleTimeoutRef.current) {
        clearTimeout(throttleTimeoutRef.current);
      }
    };
  }, []);

  // Fetch the current queue status (fallback for initial load)
  const fetchQueueStatus = useCallback(async () => {
    try {
      const status = await getDownloadQueue();
      setQueueStatus(status);
    } catch (err) {
      console.error('Failed to fetch queue status:', err);
    }
  }, []);

  // Clear progress manually
  const clearProgress = useCallback(() => {
    setProgress(null);
  }, []);

  // Cancel download
  const cancelDownload = useCallback(async (modelId: string): Promise<void> => {
    try {
      await cancelDownloadService(modelId);
      // Clear progress on successful cancel
      setProgress(null);
      setError(null);
      await fetchQueueStatus();
    } catch (err) {
      // Set error state on failure so callers can check/display it
      const message = err instanceof Error ? err.message : 'Failed to cancel download';
      setError(message);
      throw err; // Re-throw so callers can handle it
    }
  }, [fetchQueueStatus]);

  // Initial fetch of queue status
  useEffect(() => {
    fetchQueueStatus();
  }, [fetchQueueStatus]);

  // Listen for download events from Tauri or Web SSE
  useEffect(() => {
    let unlistenTauri: (() => void) | undefined;
    let unlistenTauriQueue: (() => void) | undefined;
    let eventSource: EventSource | undefined;

    const setupListener = async () => {
      if (isTauriApp) {
        setConnectionMode('Desktop (Tauri)');
        try {
          const { listen } = await import('@tauri-apps/api/event');
          // Listen for progress events (legacy format)
          unlistenTauri = await listen<DownloadProgress>('download-progress', (event) => {
            const progressData = event.payload;
            throttledSetProgress(progressData);

            if (progressData.status === 'completed') {
              onCompleted?.();
              setTimeout(() => {
                setProgress(null);
              }, 2000);
            }
          });
          
          // Listen for queue snapshot events
          interface TauriQueueSnapshot {
            items: DownloadSummary[];
            max_size: number;
          }
          unlistenTauriQueue = await listen<TauriQueueSnapshot>('download-queue-snapshot', (event) => {
            const snapshot = event.payload;
            setQueueStatus(snapshotToQueueStatus(snapshot.items, snapshot.max_size));
          });
        } catch (e) {
          console.error('[useDownloadProgress] Failed to setup Tauri listener:', e);
          setConnectionMode(`Desktop Error: ${e instanceof Error ? e.message : String(e)}`);
        }
      } else {
        setConnectionMode('Web (SSE)');
        const baseUrl = import.meta.env.DEV ? 'http://localhost:9887' : '';
        eventSource = new EventSource(`${baseUrl}/api/models/download/progress`);

        eventSource.onmessage = (event) => {
          try {
            if (!event.data || event.data.trim() === '') {
              return;
            }
            const downloadEvent = JSON.parse(event.data) as DownloadEvent;
            handleDownloadEvent(downloadEvent);
          } catch (e) {
            console.error('Failed to parse download event:', e);
          }
        };

        eventSource.onerror = (err) => {
          console.error('SSE Error:', err);
        };
      }
    };

    setupListener();

    return () => {
      if (unlistenTauri) {
        unlistenTauri();
      }
      if (unlistenTauriQueue) {
        unlistenTauriQueue();
      }
      if (eventSource) {
        eventSource.close();
      }
    };
  }, [throttledSetProgress, handleDownloadEvent, onCompleted]);

  const isDownloading = queueStatus?.current !== null && queueStatus?.current !== undefined;
  const queueCount = (queueStatus?.pending.length || 0) + (isDownloading ? 1 : 0);

  return {
    progress,
    queueStatus,
    connectionMode,
    error,
    setError,
    clearProgress,
    fetchQueueStatus,
    cancelDownload,
    isDownloading,
    queueCount,
  };
}
