/**
 * Tags API module.
 * Handles tag CRUD and model-tag associations.
 */

import { get, post, del } from './client';
import type { ModelId } from '../types/ids';

/**
 * List all available tags.
 */
export async function listTags(): Promise<string[]> {
  return get<string[]>('/api/tags');
}

/**
 * Get tags assigned to a specific model.
 */
export async function getModelTags(modelId: ModelId): Promise<string[]> {
  return get<string[]>(`/api/models/${modelId}/tags`);
}

/**
 * Add a tag to a model (creates tag if it doesn't exist).
 */
export async function addModelTag(modelId: ModelId, tag: string): Promise<void> {
  await post<void>(`/api/models/${modelId}/tags`, { tag });
}

/**
 * Remove a tag from a model.
 */
export async function removeModelTag(modelId: ModelId, tag: string): Promise<void> {
  await del<void>(`/api/models/${modelId}/tags/${encodeURIComponent(tag)}`);
}
