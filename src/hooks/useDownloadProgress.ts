import { useState, useEffect, useCallback, useRef } from 'react';
import { TauriService } from '../services/tauri';
import { isTauriApp } from '../utils/platform';
import { DownloadQueueStatus } from '../types';

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

  // Cleanup throttle timeout on unmount
  useEffect(() => {
    return () => {
      if (throttleTimeoutRef.current) {
        clearTimeout(throttleTimeoutRef.current);
      }
    };
  }, []);

  // Fetch the current queue status
  const fetchQueueStatus = useCallback(async () => {
    try {
      const status = await TauriService.getDownloadQueue();
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
  // Returns true on success, false on failure
  const cancelDownload = useCallback(async (modelId: string): Promise<void> => {
    try {
      await TauriService.cancelDownload(modelId);
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

  // Initial fetch and periodic refresh of queue status
  useEffect(() => {
    fetchQueueStatus();
    const interval = setInterval(fetchQueueStatus, 2000);
    return () => clearInterval(interval);
  }, [fetchQueueStatus]);

  // Listen for download progress events from Tauri or Web SSE
  useEffect(() => {
    let unlistenTauri: (() => void) | undefined;
    let eventSource: EventSource | undefined;

    const setupListener = async () => {
      if (isTauriApp) {
        setConnectionMode('Desktop (Tauri)');
        try {
          const { listen } = await import('@tauri-apps/api/event');
          unlistenTauri = await listen<DownloadProgress>('download-progress', (event) => {
            const progressData = event.payload;
            throttledSetProgress(progressData);
            // Note: fetchQueueStatus is handled by the 2-second interval polling,
            // not called on every progress event to avoid overwhelming the UI

            if (progressData.status === 'completed') {
              onCompleted?.();
              setTimeout(() => {
                setProgress(null);
              }, 2000);
            }
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
            const progressData = JSON.parse(event.data) as DownloadProgress;
            throttledSetProgress(progressData);
            // Note: fetchQueueStatus is handled by the 2-second interval polling,
            // not called on every progress event to avoid overwhelming the UI

            if (progressData.status === 'completed') {
              onCompleted?.();
              setTimeout(() => {
                setProgress(null);
              }, 2000);
            }
          } catch (e) {
            console.error('Failed to parse progress event', e);
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
      if (eventSource) {
        eventSource.close();
      }
    };
  }, [throttledSetProgress, onCompleted]);

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
