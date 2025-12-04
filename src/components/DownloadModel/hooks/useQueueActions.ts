import { useCallback } from "react";
import type { DownloadQueueItem } from "../../../types";
import type { QueueDownloadResponse } from "../../../services/tauri/download";

/**
 * Dependencies for useQueueActions hook.
 * Uses dependency injection to keep the hook UI-agnostic.
 */
export interface UseQueueActionsDeps {
  removeFromDownloadQueue: (modelId: string) => Promise<string>;
  cancelShardGroup: (groupId: string) => Promise<string>;
  clearFailedDownloads: () => Promise<string>;
  queueDownload: (modelId: string, quantization?: string) => Promise<QueueDownloadResponse>;
  cancelDownload: (modelId: string) => Promise<void>;
  fetchQueueStatus: () => Promise<void>;
  setError: (msg: string | null) => void;
}

/**
 * Queue action handlers returned by useQueueActions.
 */
export interface QueueActionsHandlers {
  handleRemoveFromQueue: (modelId: string) => Promise<void>;
  handleCancelShardGroup: (groupId: string) => Promise<void>;
  handleClearFailed: () => Promise<void>;
  handleRetry: (item: DownloadQueueItem) => Promise<void>;
  handleCancel: (modelId: string) => Promise<void>;
}

/**
 * Hook that wraps low-level queue operations with error handling.
 * 
 * Responsibilities:
 * - Wraps each queue action in try/catch
 * - Sets user-friendly error messages on failure
 * - Refreshes queue status after successful operations
 */
export function useQueueActions({
  removeFromDownloadQueue,
  cancelShardGroup,
  clearFailedDownloads,
  queueDownload,
  cancelDownload,
  fetchQueueStatus,
  setError,
}: UseQueueActionsDeps): QueueActionsHandlers {

  const handleRemoveFromQueue = useCallback(async (modelId: string) => {
    try {
      await removeFromDownloadQueue(modelId);
      await fetchQueueStatus();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to remove from queue");
    }
  }, [removeFromDownloadQueue, fetchQueueStatus, setError]);

  const handleCancelShardGroup = useCallback(async (groupId: string) => {
    try {
      await cancelShardGroup(groupId);
      await fetchQueueStatus();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to cancel shard group");
    }
  }, [cancelShardGroup, fetchQueueStatus, setError]);

  const handleClearFailed = useCallback(async () => {
    try {
      await clearFailedDownloads();
      await fetchQueueStatus();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to clear failed downloads");
    }
  }, [clearFailedDownloads, fetchQueueStatus, setError]);

  const handleRetry = useCallback(async (item: DownloadQueueItem) => {
    try {
      // First remove from failed, then re-queue
      // The id is in format "model_id:quantization" or just "model_id"
      await removeFromDownloadQueue(item.id);
      // Parse id to extract model_id and quantization
      const parts = item.id.split(':');
      const modelId = parts[0];
      const quantization = parts.length > 1 ? parts[1] : undefined;
      await queueDownload(modelId, quantization);
      await fetchQueueStatus();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to retry download");
    }
  }, [removeFromDownloadQueue, queueDownload, fetchQueueStatus, setError]);

  const handleCancel = useCallback(async (modelId: string) => {
    try {
      await cancelDownload(modelId);
      // cancelDownload from useDownloadProgress handles state updates
    } catch (err) {
      // Error is already set by cancelDownload, but log for debugging
      console.error("Cancel download failed:", err);
    }
  }, [cancelDownload]);

  return {
    handleRemoveFromQueue,
    handleCancelShardGroup,
    handleClearFailed,
    handleRetry,
    handleCancel,
  };
}

/** Type for the return value of useQueueActions */
export type QueueActionsController = ReturnType<typeof useQueueActions>;
