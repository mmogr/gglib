import { useState, useEffect, useCallback } from 'react';
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
  const cancelDownload = useCallback(async (modelId: string) => {
    try {
      await TauriService.cancelDownload(modelId);
      setError('Download cancelled');
      setProgress(null);
      await fetchQueueStatus();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to cancel download');
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
            setProgress(progressData);
            fetchQueueStatus();

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
            setProgress(progressData);
            fetchQueueStatus();

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
  }, [fetchQueueStatus, onCompleted]);

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
