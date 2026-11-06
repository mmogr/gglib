/**
 * Tags API module.
 * Handles tag CRUD and model-tag associations.
 */

import { post, del } from './client';
import type { ModelId } from '../types/ids';
import type { AddTagRequest } from '../../../types/generated/AddTagRequest';

/**
 * Add a tag to a model (creates tag if it doesn't exist).
 */
export async function addModelTag(modelId: ModelId, tag: string): Promise<void> {
  const body: AddTagRequest = { tag };
  await post<void>(`/api/models/${modelId}/tags`, body);
}

/**
 * Remove a tag from a model.
 */
export async function removeModelTag(modelId: ModelId, tag: string): Promise<void> {
  await del<void>(`/api/models/${modelId}/tags/${encodeURIComponent(tag)}`);
}
