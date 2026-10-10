/**
 * Local models API module.
 * Handles local model CRUD operations, search, filtering, and system info.
 */

import { del, get, patch, post, put } from '../client';
import { TransportError } from '../../errors';
import type { ModelId } from '../../types/ids';
import type { AddModelParams, UpdateModelBody, UpdateModelParams } from '../../types/models';
import type {
  ComponentChoices,
  GgufModel,
  ModelDetail,
  ModelFilterOptions,
  ModelsDirectoryInfo,
  RetagResponse,
  SamplingExplanation,
  SetCapabilitiesRequest,
  SystemMemoryInfo,
  UpgradeCheck,
  UpgradeOutcome,
} from '../../../../types';
import type { AddModelRequest } from '../../../../types/generated/AddModelRequest';
import type { ModelListQueryParams } from '../../../../types/generated/ModelListQueryParams';
import type { ProjectorChoice } from '../../../../types/generated/ProjectorChoice';
import type { RemoveModelRequest } from '../../../../types/generated/RemoveModelRequest';
import type { RetagBody } from '../../../../types/generated/RetagBody';
import type { UpdateModelsDirectoryRequest } from '../../../../types/generated/UpdateModelsDirectoryRequest';

/**
 * The sort and filters `GET /api/models` reads, by the daemon's own names.
 * A key left out, or `null`, is a filter that is not set.
 */
export type ModelListQuery = Partial<ModelListQueryParams>;

/**
 * List local models, sorted and narrowed by `query` when one is given.
 */
export async function listModels(query: ModelListQuery = {}): Promise<GgufModel[]> {
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(query)) {
    if (value !== null && value !== undefined) params.set(key, String(value));
  }
  const search = params.toString();
  return get<GgufModel[]>(search ? `/api/models?${search}` : '/api/models');
}

/**
 * Get a specific model by ID.
 * Returns null if not found (instead of throwing).
 */
export async function getModel(id: ModelId): Promise<GgufModel | null> {
  try {
    return await get<GgufModel>(`/api/models/${id}`);
  } catch (error) {
    if (TransportError.hasCode(error, 'NOT_FOUND')) {
      return null;
    }
    throw error;
  }
}

/**
 * Get full detail for a specific model by ID.
 * Returns a superset of GgufModel with HuggingFace provenance, timestamps, and raw GGUF metadata.
 * Returns null if not found (instead of throwing).
 */
export async function getModelDetail(id: ModelId): Promise<ModelDetail | null> {
  try {
    return await get<ModelDetail>(`/api/models/${id}/detail`);
  } catch (error) {
    if (TransportError.hasCode(error, 'NOT_FOUND')) {
      return null;
    }
    throw error;
  }
}

/**
 * Get a model's resolved sampling parameters and the layer that supplied each.
 *
 * `profile` names a configured inference profile to apply on top of the
 * model's own defaults; an unknown name is a 400 from the server rather than a
 * silent fall back to the unprofiled resolution.
 *
 * Returns null if the model is not found (instead of throwing).
 */
export async function explainModelSampling(
  id: ModelId,
  profile?: string,
): Promise<SamplingExplanation | null> {
  const query = profile ? `?profile=${encodeURIComponent(profile)}` : '';
  try {
    return await get<SamplingExplanation>(`/api/models/${id}/explain${query}`);
  } catch (error) {
    if (TransportError.hasCode(error, 'NOT_FOUND')) {
      return null;
    }
    throw error;
  }
}

/**
 * Add a new model from a local file.
 */
export async function addModel(params: AddModelParams): Promise<GgufModel> {
  const body: AddModelRequest = { file_path: params.filePath };
  return post<GgufModel>('/api/models', body);
}

/**
 * Remove a model.
 */
export async function removeModel(id: ModelId): Promise<void> {
  const body: RemoveModelRequest = { force: false };
  await del<void>(`/api/models/${id}`, body);
}

/**
 * Update model metadata.
 */
export async function updateModel(params: UpdateModelParams): Promise<GgufModel> {
  const body: UpdateModelBody = {
    name: params.name,
    quantization: params.quantization,
    filePath: params.filePath,
    inferenceDefaults: params.inferenceDefaults,
    serverDefaults: params.serverDefaults,
    projectorPath: params.projectorPath,
    components: params.components,
  };
  return put<GgufModel>(`/api/models/${params.id}`, body);
}

/**
 * The projector files a model's picker offers: every projector some model is
 * linked to, and this model's own files named as projectors. "None" is the
 * picker's own entry; the pick is sent back as `updateModel`'s `projectorPath`.
 */
export async function listProjectorChoices(id: ModelId): Promise<ProjectorChoice[]> {
  return get<ProjectorChoice[]>(`/api/models/${id}/projectors`);
}

/**
 * The component files an image model's pickers offer, one list for each role
 * its family needs, in the recipe's order: the files models of the family
 * link in that role, this model's own link included. Empty for a model that
 * chats. "None" is each picker's own entry; a pick is sent back in
 * `updateModel`'s `components`.
 */
export async function listComponentChoices(id: ModelId): Promise<ComponentChoices[]> {
  return get<ComponentChoices[]>(`/api/models/${id}/components`);
}

/**
 * Re-run capability detection over a model's stored metadata
 * (`gglib model retag`). `full` rebuilds the system-tag namespace.
 */
export async function retagModel(modelId: number, full = false): Promise<RetagResponse> {
  const body: RetagBody = { full };
  return post<RetagResponse>(`/api/models/${modelId}/retag`, body);
}

/** Set or clear a model's capability flags. Returns the updated model. */
export async function setModelCapabilities(
  modelId: number,
  request: SetCapabilitiesRequest,
): Promise<GgufModel> {
  return patch<GgufModel>(`/api/models/${modelId}/capabilities`, request);
}

/** Commit-SHA update check: the one `gglib model upgrade` runs before it downloads. */
export async function checkModelUpgrade(modelId: number): Promise<UpgradeCheck> {
  return get<UpgradeCheck>(`/api/models/${modelId}/upgrade-check`);
}

/**
 * Re-download at the latest HuggingFace revision (`gglib model upgrade`).
 * Blocking for the download's duration — callers show a busy state.
 */
export async function upgradeModel(modelId: number): Promise<UpgradeOutcome> {
  return post<UpgradeOutcome>(`/api/models/${modelId}/upgrade`, null);
}


/**
 * Get available filter options (tags, quantizations, parameter ranges).
 */
export async function getModelFilterOptions(): Promise<ModelFilterOptions> {
  return get<ModelFilterOptions>('/api/models/filter-options');
}

/**
 * Get system memory information.
 */
export async function getSystemMemory(): Promise<SystemMemoryInfo | null> {
  return get<SystemMemoryInfo>('/api/config/system/memory');
}

/**
 * Get models directory information.
 */
export async function getModelsDirectory(): Promise<ModelsDirectoryInfo> {
  return get<ModelsDirectoryInfo>('/api/config/system/models-directory');
}

/**
 * Set models directory path.
 */
export async function setModelsDirectory(path: string): Promise<void> {
  const body: UpdateModelsDirectoryRequest = { path };
  await put<void>('/api/config/system/models-directory', body);
}
