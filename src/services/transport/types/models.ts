/**
 * Models transport types: what the model calls take. The shapes the daemon
 * answers with are in `src/types`.
 */

import type { ModelId } from './ids';
import type { ComponentRole, ServerConfig, SparseInferenceConfig } from '../../../types';
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
  /**
   * An image model's components to change, by role: a path links the role to
   * that file, `null` unlinks it, and a role left out keeps its link. Absent
   * changes no component.
   */
  components?: ComponentChanges;
}

/** Component links to change, by role: a path links, `null` unlinks, a role left out is kept. */
export type ComponentChanges = Partial<Record<ComponentRole, string | null>>;

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
