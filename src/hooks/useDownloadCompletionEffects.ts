import { useCallback, useEffect, useRef } from 'react';
import { useToastContext } from '../contexts/ToastContext';
import { createBatchWithinWindow, DEFAULT_BATCH_WINDOW_MS } from '../utils/batchWithinWindow';
import type { DownloadCompletionInfo } from '../services/transport/types/downloads';

export interface UseDownloadCompletionEffectsOptions {
  /**
   * Function to refresh the models list. Should be a stable reference
   * (wrapped in useCallback where defined) to prevent re-batching resets.
   */
  refreshModels: () => void | Promise<void>;
  
  /**
   * Batch window duration in milliseconds. Completions within this window
   * are aggregated into a single refresh + toast.
   * @default 350
   */
  windowMs?: number;
}

/**
 * Orchestration hook for download completion effects.
 * 
 * Responsibilities:
 * - Batches completion events within a time window
 * - Triggers model refresh once per batch
 * - Dispatches aggregated toast notifications
 * 
 * @returns A stable onCompleted callback to pass to useDownloadManager
 * 
 * @example
 * const { onCompleted } = useDownloadCompletionEffects({
 *   refreshModels: handleRefreshAll,
 * });
 * 
 * const { ... } = useDownloadManager({ onCompleted });
 */
export function useDownloadCompletionEffects(
  options: UseDownloadCompletionEffectsOptions
): { onCompleted: (info: DownloadCompletionInfo) => void } {
  const { refreshModels, windowMs = DEFAULT_BATCH_WINDOW_MS } = options;
  const { showToast } = useToastContext();

  // Use refs for callbacks to avoid recreating the batcher on every render
  const refreshModelsRef = useRef(refreshModels);
  refreshModelsRef.current = refreshModels;
  
  const showToastRef = useRef(showToast);
  showToastRef.current = showToast;

  // Create batcher once and store in ref
  const batcherRef = useRef<ReturnType<typeof createBatchWithinWindow<DownloadCompletionInfo>> | null>(null);
  
  // Initialize batcher on mount
  useEffect(() => {
    const handleFlush = (items: DownloadCompletionInfo[]) => {
      // Trigger single refresh for all completions in batch
      refreshModelsRef.current();
      
      // Show aggregated toast
      if (items.length === 1) {
        const item = items[0];
        const name = item.displayName ?? item.modelId;
        showToastRef.current(`Downloaded ${name}`, 'success');
      } else {
        showToastRef.current(`${items.length} models downloaded`, 'success');
      }
    };

    batcherRef.current = createBatchWithinWindow<DownloadCompletionInfo>(windowMs, handleFlush);

    // Cleanup on unmount - dispose to clear any pending timers
    return () => {
      batcherRef.current?.dispose();
      batcherRef.current = null;
    };
  }, [windowMs]);

  // Stable callback to pass to useDownloadManager
  const onCompleted = useCallback((info: DownloadCompletionInfo) => {
    batcherRef.current?.push(info);
  }, []);

  return { onCompleted };
}
