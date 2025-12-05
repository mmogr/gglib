// Public API surface for Tauri service layer
// Import from "@/services/tauri" to get all functions

// Re-export utilities
export { isTauriApp, apiFetch, type ApiResponse } from "./base";

// Re-export domain modules
export {
  listModels,
  getModel,
  addModel,
  removeModel,
  updateModel,
  searchModels,
  getModelFilterOptions,
} from "./model";

export {
  serveModel,
  stopServer,
  listServers,
  type ServeResponse,
} from "./server";

export {
  getProxyStatus,
  startProxy,
  stopProxy,
  type ProxyConfig,
  type ProxyStatus,
} from "./proxy";

export {
  listTags,
  addModelTag,
  removeModelTag,
  getModelTags,
} from "./tags";

export {
  browseHfModels,
  getHfQuantizations,
  getHfToolSupport,
} from "./huggingface";

export {
  openUrl,
  setSelectedModel,
  syncMenuState,
  syncMenuStateSilent,
} from "./system";
