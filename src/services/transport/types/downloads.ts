/**
 * Downloads transport types.
 * Handles download queue management.
 */

import type { HfModelId } from './ids';

/**
 * The download queue as the daemon serves it, on `GET
 * /api/models/downloads/queue` and in every `queue_snapshot` event alike.
 *
 * The GUI holds this as its download state and prints each row's `text`. It
 * does not group, label or format a download itself; the one number it works
 * out is how many rows are waiting.
 */
export type { QueueSnapshot } from '../../../types/generated/QueueSnapshot';
export type { DownloadRow } from '../../../types/generated/DownloadRow';
export type { DownloadRowText } from '../../../types/generated/DownloadRowText';
export type { DownloadPhase } from '../../../types/generated/DownloadPhase';
export type { FinishedDownload } from '../../../types/generated/FinishedDownload';
export type { DownloadOutcome } from '../../../types/generated/DownloadOutcome';

/**
 * Parameters for queueing a download.
 */
export interface QueueDownloadParams {
  modelId: HfModelId;
  /** Optional quantization. If omitted, smart selection picks the best available. */
  quantization?: string;
}

/**
 * Response from queueing a download.
 * Canonical shape returned by all transports.
 */
import type { QueueDownloadResponse } from '../../../types/generated/QueueDownloadResponse';
export type { QueueDownloadResponse };

/**
 * A download that completed, for the model refresh and the toast.
 */
export interface DownloadCompletionInfo {
  /** Canonical download ID (model_id:quantization or model_id) */
  id: string;
  /** How it ended, in the daemon's words: its finished entry's text */
  text: string;
}

/**
 * A download that failed, for the toast.
 */
export interface DownloadFailureInfo {
  /** Canonical download ID (model_id:quantization or model_id) */
  id: string;
  /** How it ended, in the daemon's words: its finished entry's text */
  text: string;
}
