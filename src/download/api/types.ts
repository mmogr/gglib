import type { DownloadEvent as LegacyDownloadEvent, DownloadQueueStatus as LegacyDownloadQueueStatus, DownloadSummary as LegacyDownloadSummary, ShardInfo as LegacyShardInfo } from '../../types';

// Canonical download identifier used by the backend (model_id:quantization or model_id).
export type DownloadId = string;

// Re-export legacy types to keep a single source of truth while migrating.
export type DownloadSummary = LegacyDownloadSummary;
export type DownloadEvent = LegacyDownloadEvent;
export type DownloadQueueStatus = LegacyDownloadQueueStatus;
export type ShardInfo = LegacyShardInfo;

// Progress map keyed by DownloadId for easy lookup in the UI.
export type ProgressById = Map<DownloadId, DownloadEvent>;
