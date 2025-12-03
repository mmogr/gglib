// Tags domain operations
// Model tagging and categorization

import { apiFetch, isTauriApp, tauriInvoke, ApiResponse } from "./base";

/**
 * List all unique tags in the library.
 */
export async function listTags(): Promise<string[]> {
  if (isTauriApp) {
    return await tauriInvoke<string[]>('list_tags');
  } else {
    const response = await apiFetch(`/tags`);
    if (!response.ok) {
      throw new Error(`Failed to fetch tags: ${response.statusText}`);
    }
    const data: ApiResponse<string[]> = await response.json();
    return data.data || [];
  }
}

/**
 * Add a tag to a model.
 */
export async function addModelTag(modelId: number, tag: string): Promise<string> {
  if (isTauriApp) {
    return await tauriInvoke<string>('add_model_tag', { modelId, tag });
  } else {
    const response = await apiFetch(`/models/${modelId}/tags`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ tag }),
    });

    if (!response.ok) {
      const error: ApiResponse<unknown> = await response.json();
      throw new Error((error.error as string) || 'Failed to add tag to model');
    }

    const data: ApiResponse<string> = await response.json();
    return data.data || 'Tag added to model successfully';
  }
}

/**
 * Remove a tag from a model.
 */
export async function removeModelTag(modelId: number, tag: string): Promise<string> {
  if (isTauriApp) {
    return await tauriInvoke<string>('remove_model_tag', { modelId, tag });
  } else {
    const response = await apiFetch(`/models/${modelId}/tags`, {
      method: 'DELETE',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ tag }),
    });

    if (!response.ok) {
      const error: ApiResponse<unknown> = await response.json();
      throw new Error((error.error as string) || 'Failed to remove tag from model');
    }

    const data: ApiResponse<string> = await response.json();
    return data.data || 'Tag removed from model successfully';
  }
}

/**
 * Get all tags for a specific model.
 */
export async function getModelTags(modelId: number): Promise<string[]> {
  if (isTauriApp) {
    return await tauriInvoke<string[]>('get_model_tags', { modelId });
  } else {
    const response = await apiFetch(`/models/${modelId}/tags`);
    if (!response.ok) {
      throw new Error(`Failed to fetch model tags: ${response.statusText}`);
    }
    const data: ApiResponse<string[]> = await response.json();
    return data.data || [];
  }
}
