/**
 * Verification transport sub-interface.
 * Handles model integrity verification and repair operations.
 */

import type { ModelId } from './ids';
import type { OverallHealth } from './events';

/** Health status of an individual shard */
export type ShardHealth =
  | { type: 'healthy' }
  | { type: 'corrupt'; expected: string; actual: string }
  | { type: 'missing' }
  | { type: 'no_oid' };

/** Health report for a single shard */
export interface ShardHealthReport {
  index: number;
  file_path: string;
  health: ShardHealth;
}

/** Complete verification report */
export interface VerificationReport {
  model_id: number;
  overall_health: OverallHealth;
  shards: ShardHealthReport[];
  verified_at: string;
}

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
