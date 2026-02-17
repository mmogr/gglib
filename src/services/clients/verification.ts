/**
 * Verification client module.
 *
 * API client for model verification operations.
 * Delegates to the Transport layer.
 *
 * @module services/clients/verification
 */

import { getTransport } from '../transport';
import type { ModelId } from '../transport/types/ids';
import type {
  ShardHealth,
  ShardHealthReport,
  VerificationReport,
  UpdateCheckResult,
} from '../transport/types/verification';
import type { OverallHealth } from '../transport/types/events';

// Re-export types from transport layer for convenience
export type { ShardHealth, ShardHealthReport, VerificationReport, UpdateCheckResult, OverallHealth };

/**
 * Verify the integrity of a model by computing SHA256 hashes.
 * Progress updates are streamed via SSE (subscribe to 'verification' events).
 */
export async function verifyModel(modelId: ModelId): Promise<VerificationReport> {
  return getTransport().verifyModel(modelId);
}

/**
 * Check if updates are available for a model on HuggingFace.
 */
export async function checkModelUpdates(modelId: ModelId): Promise<UpdateCheckResult> {
  return getTransport().checkModelUpdates(modelId);
}

/**
 * Repair a model by re-downloading corrupt shards.
 * 
 * @param modelId - ID of the model to repair
 * @param shards - Optional list of shard indices to repair. If not specified, repairs all corrupt shards.
 */
export async function repairModel(modelId: ModelId, shards?: number[]): Promise<{ message: string }> {
  return getTransport().repairModel(modelId, shards);
}
