/**
 * Models transport sub-interface.
 * Handles model CRUD, filtering, and HuggingFace browsing.
 */

import type { ModelId, HfModelId } from './ids';
import type {
  GgufModel,
  ModelDetail,
  ServeConfig,
  ModelsDirectoryInfo,
  SystemMemoryInfo,
  FitStatus,
  HfModelSummary,
  HfSearchRequest,
  HfSearchResponse,
  HfQuantization,
  HfQuantizationsResponse,
  ToolSupportResponse,
  HfSortField,
  ModelFilterOptions,
  RangeValues,
  RetagResponse,
  SetCapabilitiesRequest,
  UpgradeCheck,
  UpgradeOutcome,
} from '../../../types';

// Re-export existing types that clients already use
export type {
  GgufModel,
  ModelDetail,
  ServeConfig,
  ModelsDirectoryInfo,
  SystemMemoryInfo,
  FitStatus,
  HfModelSummary,
  HfSearchRequest,
  HfSearchResponse,
  HfQuantization,
  HfQuantizationsResponse,
  ToolSupportResponse,
  HfSortField,
  ModelFilterOptions,
  RangeValues,
  RetagResponse,
  SetCapabilitiesRequest,
  UpgradeCheck,
  UpgradeOutcome,
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
  quantization?: string;
  filePath?: string;
  inferenceDefaults?: import('../../../types').SparseInferenceConfig;
  serverDefaults?: import('../../../types').ServerConfig | null;
}

/**
 * Models transport operations.
 */
export interface ModelsTransport {
  // Local model CRUD
  listModels(): Promise<GgufModel[]>;
  getModel(id: ModelId): Promise<GgufModel | null>;
  getModelDetail(id: ModelId): Promise<ModelDetail | null>;
  explainModelSampling(
    id: ModelId,
    profile?: string,
  ): Promise<import('../../../types').SamplingExplanation | null>;
  addModel(params: AddModelParams): Promise<GgufModel>;
  removeModel(id: ModelId): Promise<void>;
  updateModel(params: UpdateModelParams): Promise<GgufModel>;

  // Filtering
  /** Re-run capability detection (`gglib model retag`). */
  retagModel(modelId: number, full?: boolean): Promise<RetagResponse>;
  /** Set or clear capability flags. Returns the updated model. */
  setModelCapabilities(modelId: number, request: SetCapabilitiesRequest): Promise<GgufModel>;
  /** Commit-SHA update check for one model. */
  checkModelUpgrade(modelId: number): Promise<UpgradeCheck>;
  /** Re-download at the latest HuggingFace revision. Blocking. */
  upgradeModel(modelId: number): Promise<UpgradeOutcome>;
  getModelFilterOptions(): Promise<ModelFilterOptions>;

  // HuggingFace browsing
  browseHfModels(params: HfSearchRequest): Promise<HfSearchResponse>;
  getHfModelSummary(modelId: HfModelId): Promise<HfModelSummary>;
  getHfQuantizations(modelId: HfModelId): Promise<HfQuantizationsResponse>;
  getHfToolSupport(modelId: HfModelId): Promise<ToolSupportResponse>;

  // System info
  getSystemMemory(): Promise<SystemMemoryInfo | null>;
  getModelsDirectory(): Promise<ModelsDirectoryInfo>;
  setModelsDirectory(path: string): Promise<void>;
}
