/**
 * Verification transport sub-interface.
 * Handles model integrity verification and repair operations.
 */

import type { ModelId } from './ids';

// Imported too, because the transport interface below names them.
import type { VerificationReport } from '../../../types/generated/VerificationReport';
export type { VerificationReport };
import type { UpdateCheckResult } from '../../../types/generated/UpdateCheckResult';
export type { UpdateCheckResult };
export type { CheckUpdatesResponse } from '../../../types/generated/CheckUpdatesResponse';

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
