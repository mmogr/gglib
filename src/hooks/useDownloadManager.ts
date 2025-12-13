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
import type { DownloadEvent, DownloadSummary } from '../services/transport/types/events';
import { isDesktop } from '../services/platform';

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

export interface UseDownloadManagerResult {
  queueStatus: DownloadQueueStatus | null;
  currentProgress: DownloadProgressView | null;
  isDownloading: boolean;
  queueLength: number;
  connectionMode: string;
  error: string | null;
  setError: (msg: string | null) => void;
  refreshQueue: () => Promise<void>;
  queueModel: (modelId: string, quantization?: string) => Promise<QueueDownloadResponse>;
  cancel: (id: string) => Promise<void>;
  cancelGroup: (groupId: string) => Promise<void>;
  clearFailed: () => Promise<void>;
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
  const [connectionMode, setConnectionMode] = useState<string>('Initializing...');
  const [error, setError] = useState<string | null>(null);

  const lastProgressUpdateRef = useRef<number>(0);
  const pendingProgressRef = useRef<DownloadProgressView | null>(null);
  const throttleTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  
  // Ref to access current queue status in event handler without causing re-subscriptions
  const queueStatusRef = useRef<DownloadQueueStatus | null>(null);
  queueStatusRef.current = queueStatus;

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
        setQueueStatus(snapshotToQueueStatus(event.items, event.max_size));
        return;
      }

      const progress = eventToProgress(event);
      if (progress) {
        throttledSetProgressRef.current(progress);

        if (progress.status === 'completed' && event.type === 'download_completed') {
          // Extract completion info from event for UI effects layer
          const completionInfo = extractCompletionInfo(event.id, queueStatusRef.current);
          onCompletedRef.current?.(completionInfo);
          setTimeout(() => setCurrentProgress(null), 2000);
        }
      }
    };

    // Subscribe to download events via Transport (sync unsubscribe)
    const unsubscribe = subscribeToEvent('download', handleEvent);
    setConnectionMode(isDesktop() ? 'Desktop (Tauri)' : 'Web (SSE)');

    return () => {
      unsubscribe();
    };
  }, [refreshQueue]); // Only depend on refreshQueue, not handleEvent

  const queueModel = useCallback(async (modelId: string, quantization?: string) => {
    const response = await queueDownload({ modelId, quantization });
    await refreshQueue();
    return response;
  }, [refreshQueue]);

  const cancel = useCallback(async (id: string) => {
    await cancelDownload(id);
    await refreshQueue();
  }, [refreshQueue]);

  const cancelGroup = useCallback(async (groupId: string) => {
    await cancelShardGroup(groupId);
    await refreshQueue();
  }, [refreshQueue]);

  const clearFailed = useCallback(async () => {
    await clearFailedDownloads();
    await refreshQueue();
  }, [refreshQueue]);

  const isDownloading = !!queueStatus?.current;
  const queueLength = (queueStatus?.pending?.length || 0) + (isDownloading ? 1 : 0);

  return {
    queueStatus,
    currentProgress,
    isDownloading,
    queueLength,
    connectionMode,
    error,
    setError,
    refreshQueue,
    queueModel,
    cancel,
    cancelGroup,
    clearFailed,
  };
}
