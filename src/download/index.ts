// Download Module - Public API
// Re-exports all public types, hooks, and components for the download domain

// Types
export type {
  DownloadId,
  DownloadStatus,
  ShardInfo,
  DownloadSummary,
  DownloadQueueItem,
  DownloadQueueStatus,
  DownloadEvent,
  ProgressById,
  DownloadCompletionInfo,
} from './api/types';

// API functions
export {
  queueDownload,
  cancelDownload,
  removeFromDownloadQueue,
  getQueueSnapshot,
  clearFailedDownloads,
  cancelShardGroup,
  reorderDownloadQueue,
  subscribeToDownloadEvents,
  type QueueDownloadResponse,
  type ReorderResponse,
  type DownloadEventListener,
} from './api/downloadApi';

// Hooks
export {
  useDownloadManager,
  type DownloadProgressStatus,
  type DownloadProgressView,
  type UseDownloadManagerResult,
} from './hooks/useDownloadManager';
