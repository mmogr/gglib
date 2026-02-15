/**
 * Verification API module.
 * Handles model integrity verification and repair operations.
 */

import { get, post } from './client';
import type { ModelId } from '../types/ids';
import type {
  VerificationReport,
  UpdateCheckResult,
} from '../types/verification';

/**
 * Verify the integrity of a model by computing SHA256 hashes.
 * Progress updates are streamed via SSE (subscribe to 'verification' events).
 */
export async function verifyModel(modelId: ModelId): Promise<VerificationReport> {
  const response = await post<{ report: VerificationReport }>(
    `/api/models/${modelId}/verify`,
    {}
  );
  return response.report;
}

/**
 * Check if updates are available for a model on HuggingFace.
 */
export async function checkModelUpdates(modelId: ModelId): Promise<UpdateCheckResult> {
  const response = await get<{ result: UpdateCheckResult; message: string }>(
    `/api/models/${modelId}/updates`
  );
  return response.result;
}

/**
 * Repair a model by re-downloading corrupt shards.
 * 
 * @param modelId - ID of the model to repair
 * @param shards - Optional list of shard indices to repair
 */
export async function repairModel(
  modelId: ModelId,
  shards?: number[]
): Promise<{ message: string }> {
  return post<{ message: string }>(`/api/models/${modelId}/repair`, { shards });
}
