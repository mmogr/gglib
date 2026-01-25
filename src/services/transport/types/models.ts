/**
 * Models transport sub-interface.
 * Handles model CRUD, filtering, and HuggingFace browsing.
 */

import type { ModelId, HfModelId } from './ids';
import type {
  GgufModel,
  ServeConfig,
  ModelsDirectoryInfo,
  SystemMemoryInfo,
  FitStatus,
  HfModelSummary,
  HfSearchRequest,
  HfSearchResponse,
  HfQuantization,
  HfQuantizationsResponse,
  HfToolSupportResponse,
  HfSortField,
  ModelFilterOptions,
  RangeValues,
} from '../../../types';

// Re-export existing types that clients already use
export type {
  GgufModel,
  ServeConfig,
  ModelsDirectoryInfo,
  SystemMemoryInfo,
  FitStatus,
  HfModelSummary,
  HfSearchRequest,
  HfSearchResponse,
  HfQuantization,
  HfQuantizationsResponse,
  HfToolSupportResponse,
  HfSortField,
  ModelFilterOptions,
  RangeValues,
};

/**
 * Parameters for adding a model from a local file.
 */
export interface AddModelParams {
  filePath: string;
  name?: string;
}

/**
 * Parameters for updating model metadata.
 */
export interface UpdateModelParams {
  id: ModelId;
  name?: string;
}

/**
 * Parameters for searching local models.
 */
export interface SearchModelsParams {
  query?: string;
  tags?: string[];
  quantizations?: string[];
  minParams?: number;
  maxParams?: number;
}

/**
 * Models transport operations.
 */
export interface ModelsTransport {
  // Local model CRUD
  listModels(): Promise<GgufModel[]>;
  getModel(id: ModelId): Promise<GgufModel | null>;
  addModel(params: AddModelParams): Promise<GgufModel>;
  removeModel(id: ModelId): Promise<void>;
  updateModel(params: UpdateModelParams): Promise<GgufModel>;

  // Filtering
  searchModels(params: SearchModelsParams): Promise<GgufModel[]>;
  getModelFilterOptions(): Promise<ModelFilterOptions>;

  // HuggingFace browsing
  browseHfModels(params: HfSearchRequest): Promise<HfSearchResponse>;
  getHfModelSummary(modelId: HfModelId): Promise<HfModelSummary>;
  getHfQuantizations(modelId: HfModelId): Promise<HfQuantizationsResponse>;
  getHfToolSupport(modelId: HfModelId): Promise<HfToolSupportResponse>;

  // System info
  getSystemMemory(): Promise<SystemMemoryInfo | null>;
  getModelsDirectory(): Promise<ModelsDirectoryInfo>;
  setModelsDirectory(path: string): Promise<void>;
}
