/**
 * Verification transport sub-interface.
 * Handles model integrity verification and repair operations.
 */

import type { ModelId } from './ids';

/** Health status of an individual shard */
export type { ShardHealth } from '../../../types/generated/ShardHealth';
export type { ShardHealthReport } from '../../../types/generated/ShardHealthReport';
// Imported too, because the transport interface below names it.
import type { VerificationReport } from '../../../types/generated/VerificationReport';
export type { VerificationReport };

/** Result of checking for updates */
export interface UpdateCheckResult {
  model_id: number;
  update_available: boolean;
  details?: {
    changed_shards: number;
    changes: Array<{
      index: number;
      file_path: string;
      old_oid: string;
      new_oid: string;
    }>;
  };
}

/**
 * Verification transport operations.
 */
export interface VerificationTransport {
  /**
   * Verify the integrity of a model by computing SHA256 hashes.
   * Progress updates are streamed via SSE (subscribe to 'verification' events).
   */
  verifyModel(modelId: ModelId): Promise<VerificationReport>;

  /**
   * Check if updates are available for a model on HuggingFace.
   */
  checkModelUpdates(modelId: ModelId): Promise<UpdateCheckResult>;

  /**
   * Repair a model by re-downloading corrupt shards.
   * 
   * @param modelId - ID of the model to repair
   * @param shards - Optional list of shard indices to repair
   */
  repairModel(modelId: ModelId, shards?: number[]): Promise<{ message: string }>;
}
