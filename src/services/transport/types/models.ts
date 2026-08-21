/**
 * Models transport types.
 * Handles model CRUD, filtering, and HuggingFace browsing.
 */

import type { ModelId } from './ids';
import type {
  GgufModel,
  ModelDetail,
  ServeConfig,
  ModelsDirectoryInfo,
  SystemMemoryInfo,
  FitStatus,
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
