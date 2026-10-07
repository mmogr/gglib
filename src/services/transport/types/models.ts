/**
 * Models transport types: what the model calls take. The shapes the daemon
 * answers with are in `src/types`.
 */

import type { ModelId } from './ids';
import type { ServerConfig, SparseInferenceConfig } from '../../../types';
import type { UpdateModelRequest } from '../../../types/generated/UpdateModelRequest';

/**
 * Parameters for adding a model from a local file.
 */
export interface AddModelParams {
  filePath: string;
}

/**
 * Parameters for updating model metadata.
 */
export interface UpdateModelParams {
  id: ModelId;
  name?: string;
  quantization?: string;
  filePath?: string;
  inferenceDefaults?: SparseInferenceConfig;
  serverDefaults?: ServerConfig | null;
  /** A path links the model to that projector, `null` unlinks it, absent leaves the link alone. */
  projectorPath?: string | null;
}

/**
 * `PUT /api/models/{id}` body: an absent key leaves that field alone.
 *
 * `Partial` because every field of the Rust struct is an `Option`, which
 * serde reads as `None` from a missing key. `inferenceDefaults` takes its
 * sparse form for the reason `UpdateSettingsRequest` does: the form sends the
 * parameters it touched, not all eighteen.
 */
export type UpdateModelBody = Omit<Partial<UpdateModelRequest>, 'inferenceDefaults'> & {
  inferenceDefaults?: SparseInferenceConfig;
};
