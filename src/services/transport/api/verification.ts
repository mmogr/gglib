/**
 * Verification API module.
 * Handles model integrity verification and repair operations.
 */

import { get, post } from './client';
import type { ModelId } from '../types/ids';
import type {
  VerificationReport,
  UpdateCheckResult,
  CheckUpdatesResponse,
} from '../types/verification';
import type { RepairRequest } from '../../../types/generated/RepairRequest';
import type { RepairStarted } from '../../../types/generated/RepairStarted';

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
  const response = await get<CheckUpdatesResponse>(`/api/models/${modelId}/updates`);
  return response.result;
}

/**
 * Repair a model by re-downloading corrupt shards.
 *
 * The daemon answers once the files are deleted and their download is queued
 * and started: that download's id, a row of the download queue from then on,
 * and the files it is to bring back.
 *
 * @param modelId - ID of the model to repair
 * @param shards - Optional list of shard indices to repair
 */
export async function repairModel(
  modelId: ModelId,
  shards?: number[]
): Promise<RepairStarted> {
  // An absent `shards` is `None` to the daemon: repair every corrupt shard.
  const body: Partial<RepairRequest> = { shards };
  return post<RepairStarted>(`/api/models/${modelId}/repair`, body);
}
