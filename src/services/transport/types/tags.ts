/**
 * Tags transport sub-interface.
 * Handles tag CRUD and model-tag associations.
 */

import type { ModelId } from './ids';

/**
 * Tags transport operations.
 */
export interface TagsTransport {
  /** List all available tags. */
  listTags(): Promise<string[]>;

  /** Get tags assigned to a specific model. */
  getModelTags(modelId: ModelId): Promise<string[]>;

  /** Add a tag to a model. Creates the tag if it doesn't exist. */
  addModelTag(modelId: ModelId, tag: string): Promise<void>;

  /** Remove a tag from a model. */
  removeModelTag(modelId: ModelId, tag: string): Promise<void>;
}
