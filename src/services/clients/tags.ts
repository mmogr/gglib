/**
 * Tags client module.
 * 
 * Thin wrapper that delegates to the Transport layer.
 * Platform-agnostic: transport selection happens once at composition root.
 * 
 * @module services/clients/tags
 */

import { getTransport } from '../transport';
import type { ModelId } from '../transport/types/ids';

/**
 * List all unique tags in the library.
 */
export async function listTags(): Promise<string[]> {
  return getTransport().listTags();
}

/**
 * Get all tags for a specific model.
 */
export async function getModelTags(modelId: ModelId): Promise<string[]> {
  return getTransport().getModelTags(modelId);
}

/**
 * Add a tag to a model. Creates the tag if it doesn't exist.
 */
export async function addModelTag(modelId: ModelId, tag: string): Promise<void> {
  return getTransport().addModelTag(modelId, tag);
}

/**
 * Remove a tag from a model.
 */
export async function removeModelTag(modelId: ModelId, tag: string): Promise<void> {
  return getTransport().removeModelTag(modelId, tag);
}
